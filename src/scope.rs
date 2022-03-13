use futures::Future;

use crate::{
    actor::{self, Actor},
    address::Address,
    errors::{DidntArrive, NoReply},
    function::ReqFn,
    messaging::Reply,
};
use std::{marker::PhantomData, mem::transmute, pin::Pin};

/// A scope for which references to a variable can be sent to processes. After the scope,
/// all replies are `.await`ed, and collected into a `Vec<Reply<R>>`. Values sent by reference
/// cannot be dropped until the end of the scope, which guarantees that the process does not
/// have access to this reference anymore. Any data returned which has a lifetime, will have the
/// lifetime of the shortest borrowed parameter that is sent in the scope.
#[macro_export]
macro_rules! scoped {
    ($function:expr) => {
        unsafe { Scope::new($function).await }
    };
}

/// A scope for which references to a variable can be sent to processes. After the scope,
/// all replies are `.await`ed, and collected into a `Vec<Reply<R>>`. Values sent by reference
/// cannot be dropped until the end of the scope, which guarantees that the process does not
/// have access to this reference anymore. Any data returned which has a lifetime, will have the
/// lifetime of the shortest borrowed parameter that is sent in the scope.
pub struct Scope<'scope, R> {
    items: Vec<ScopeItem<'scope, R>>,
    scope: PhantomData<&'scope ()>,
}

enum ScopeItem<'scope, R> {
    Reply(Reply<R>),
    MapReply(
        Box<dyn Send + 'scope>,
        usize,
        unsafe fn(
            Box<dyn Send + 'scope>,
            usize,
        ) -> Pin<Box<dyn Future<Output = Result<R, NoReply>> + Send + 'scope>>,
    ),
}

impl<'scope, R> Scope<'scope, R>
where
    R: 'scope + Send,
{
    /// Send a request that will be awaited once this scope completes. Values can be sent by
    /// reference.
    pub fn send<A: Actor, P>(
        &mut self,
        address: &Address<A>,
        req_fn: ReqFn<A, fn(P) -> R>,
        params: P,
    ) -> Result<(), DidntArrive<P>>
    where
        P: 'scope + Send,
    {
        match unsafe { address.req_ref(req_fn, params) } {
            Ok(reply) => {
                self.items.push(ScopeItem::Reply(reply));
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Create a new scope
    ///
    /// ### Safety
    /// Must be awaited after usage, which is why this is wrapped in the macro [scoped!].
    pub async unsafe fn new<T>(scope_fn: impl FnOnce(&mut Self) -> T) -> (Vec<Result<R, NoReply>>, T)
    where
        T: 'static,
        R: 'scope,
    {
        let mut scope = Scope {
            items: Vec::new(),
            scope: PhantomData,
        };

        let t = scope_fn(&mut scope);

        let mut replies = Vec::new();

        for item in scope.items {
            match item {
                ScopeItem::Reply(reply) => replies.push(reply.await),
                ScopeItem::MapReply(boxed, function, wrapper) => {
                    replies.push(wrapper(boxed, function).await)
                }
            }
        }

        (replies, t)
    }

    /// Send a request that will be awaited once this scope completes. Values can be sent by
    /// reference.
    pub fn send_map<A: Actor, P, R2, F>(
        &mut self,
        address: &Address<A>,
        req_fn: ReqFn<A, fn(P) -> R2>,
        params: P,
        map: fn(R2) -> F,
    ) -> Result<(), DidntArrive<P>>
    where
        P: 'scope + Send,
        F: Future<Output = R> + Send + 'scope,
        R2: 'scope + Send,
    {
        match unsafe { address.req_ref(req_fn, params) } {
            Ok(reply) => {
                self.items.push(ScopeItem::MapReply(
                    Box::new(reply),
                    map as usize,
                    Self::__map__::<R2, F>,
                ));
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    unsafe fn __map__<R2: Send + 'scope, F: Future<Output = R> + Send + 'scope>(
        reply: Box<dyn Send + 'scope>,
        map: usize,
    ) -> Pin<Box<dyn Future<Output = Result<R, NoReply>> + Send + 'scope>> {
        let (address, _meta) = Box::into_raw(reply).to_raw_parts();
        let reply: Box<Reply<R2>> = transmute(address);

        let map: fn(R2) -> F = transmute(map);

        Box::pin(async move {
            match reply.await {
                Ok(reply) => Ok(map(reply).await),
                Err(e) => Err(e),
            }
        })
    }
}

mod test {
    use super::*;
    use crate::{prelude::*, test::TestActor};

    #[tokio::test]
    async fn basic_scoped_test() {
        let child = actor::spawn::<TestActor>(10);
        let address = child.addr();

        let str = "hi".to_string();

        let (results, operation) = scoped!(|scope| {
            scope
                .send(&address, Fn!(TestActor::echo), &str)
                .unwrap();
            scope
                .send(&address, Fn!(TestActor::echo), &str)
                .unwrap();
            scope
                .send(&address, Fn!(TestActor::echo), &str)
                .unwrap();

            scope
                .send_map(&address, Fn!(TestActor::echo), &str, |_val| async {
                    "bye"
                })
                .unwrap();

            expensive_operation_while_waiting()
        });

        for (i, result) in results.into_iter().enumerate() {
            let result = result.unwrap();
            if i == 3 {
                assert_eq!(result, "bye")
            } else {
                assert_eq!(result, "hi")
            }
        }

        assert_eq!(operation, 10)
    }

    #[allow(dead_code)]
    fn expensive_operation_while_waiting() -> u32 {
        10
    }
}
