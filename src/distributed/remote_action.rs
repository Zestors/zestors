use std::{any::Any, pin::Pin};

use futures::Future;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    action::{Action},
    actor::{Actor, ProcessId},
    address::Addressable,
    function::{AnyFn, HandlerPtr, MsgFn},
    inbox::Unbounded,
};

use super::registry::Registry;

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteAction {
    function: AbsAnyFn,
    process_id: ProcessId,
    remote_handler: RemoteHandlerPtr,
    params: RemoteHandlerParams,
}

impl RemoteAction {
    fn __msg_handler__<'a, A: Actor, P: Send + 'static + Serialize + DeserializeOwned>(
        registry: &'a Registry,
        process_id: ProcessId,
        function: AbsAnyFn,
        params: RemoteHandlerParams,
    ) -> Pin<Box<dyn Future<Output = RemoteHandlerReturn> + Send + 'a>> {
        Box::pin(async move {
            if let Ok(address) = registry.find_by_id::<A>(process_id) {
                let params: P = params.deserialize().unwrap();
                let action: Action<A> = unsafe { Action::new_raw_msg(function.into(), params) };
                match unsafe { address.send_raw::<P>(action) } {
                    Ok(_) => RemoteHandlerReturn::Arrived,
                    Err(_) => RemoteHandlerReturn::DidntArrive,
                }
            } else {
                RemoteHandlerReturn::ProcessNotFound
            }
        })
    }

    pub fn new_msg<'a, A: Actor, P: Send + 'static + Serialize + DeserializeOwned>(
        function: MsgFn<A, P>,
        params: &P,
        process_id: ProcessId,
    ) -> Self {
        let any_fn: AnyFn = function.into();
        Self {
            process_id,
            function: any_fn.into(),
            params: RemoteHandlerParams::serialize(params),
            remote_handler: RemoteHandlerPtr::new(Self::__msg_handler__::<A, P>),
        }
    }

    /// Is safe if this action is serialized and deserialized using the same binary
    pub async unsafe fn handle(self, registry: &Registry) -> RemoteHandlerReturn {
        let remote_handler = self.remote_handler.get();
        remote_handler(registry, self.process_id, self.function, self.params).await
    }
}

trait Function<'a, A: 'a + std::fmt::Debug, P: 'a, R: 'a>: Fn(&'a mut A, &'a mut P, R) {
    fn test(a: &'a mut A, p: &'a mut P, r: R) where R: 'a {
        drop(r);
        println!("{:?}", a);
    }
}

//------------------------------------------------------------------------------------------------
//  RemoteAnyFn
//------------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct AbsAnyFn {
    function: AbsPtr,
    handler: AbsHandlerPtr,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
enum AbsHandlerPtr {
    Async(AbsPtr),
    Sync(AbsPtr),
}

impl From<AnyFn> for AbsAnyFn {
    fn from(any_fn: AnyFn) -> Self {
        let handler = match any_fn.handler {
            HandlerPtr::Sync(ptr) => AbsHandlerPtr::Sync(AbsPtr::new(ptr)),
            HandlerPtr::Async(ptr) => AbsHandlerPtr::Async(AbsPtr::new(ptr)),
        };
        Self {
            function: AbsPtr::new(any_fn.function),
            handler,
        }
    }
}

impl From<AbsAnyFn> for AnyFn {
    fn from(any_fn: AbsAnyFn) -> Self {
        let handler = match any_fn.handler {
            AbsHandlerPtr::Sync(ptr) => HandlerPtr::Sync(ptr.get()),
            AbsHandlerPtr::Async(ptr) => HandlerPtr::Async(ptr.get()),
        };
        Self {
            function: any_fn.function.get(),
            handler,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  BasePtr
//------------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub(crate) struct AbsPtr(usize);

impl AbsPtr {
    pub(crate) fn new(ptr: usize) -> Self {
        // let ptr = Self::get as usize;
        // println!("ptr {} - offset {} = {}", ptr, Self::offset(), (ptr as i128 - Self::offset() as i128));
        Self(ptr.wrapping_sub(Self::offset()))
    }

    pub(crate) fn get(self) -> usize {
        self.0.wrapping_add(Self::offset())
    }

    fn offset() -> usize {
        &() as *const () as usize
    }
}

//------------------------------------------------------------------------------------------------
//  RemoteHandlerPtr
//------------------------------------------------------------------------------------------------

// type RemoteHandlerFnType = fn(&Registry, ProcessId, AnyFn, Vec<u8>);
type RemoteHandlerFnType =
    for<'a> fn(
        &'a Registry,
        ProcessId,
        AbsAnyFn,
        RemoteHandlerParams,
    ) -> Pin<Box<dyn Future<Output = RemoteHandlerReturn> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct RemoteHandlerPtr(AbsPtr);

impl RemoteHandlerPtr {
    fn new(handler: RemoteHandlerFnType) -> Self {
        Self(AbsPtr::new(handler as usize))
    }

    fn get(self) -> RemoteHandlerFnType {
        unsafe { std::mem::transmute(self.0.get()) }
    }
}

pub enum RemoteHandlerReturn {
    Arrived,
    DidntArrive,
    ProcessNotFound,
}

//------------------------------------------------------------------------------------------------
//  RemoteHandlerParams
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RemoteHandlerParams(Vec<u8>);

impl RemoteHandlerParams {
    fn serialize<P: Serialize>(params: &P) -> Self {
        Self(bincode::serialize(params).unwrap())
    }

    fn deserialize<P: DeserializeOwned>(&self) -> Result<P, Box<bincode::ErrorKind>> {
        bincode::deserialize(&self.0)
    }
}
