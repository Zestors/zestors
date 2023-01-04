use crate::*;
use async_trait::async_trait;
use futures::Future;
use std::marker::PhantomData;
use tiny_actor::Link;

pub(super) type RestartResult<T> = Result<T, StartError>;

impl<P, E, Fun, Fut> From<ChildSpec<P, E, Fun, Fut>> for DynamicChildSpec
where
    P: Protocol + Send,
    E: Send + 'static,
    Fun: FnMut(StartOption<E>, Link) -> Fut + Send + 'static,
    Fut: Future<Output = RestartResult<Option<Child<E, P>>>> + Send + 'static,
{
    fn from(val: ChildSpec<P, E, Fun, Fut>) -> Self {
        val.into_dyn()
    }
}

//------------------------------------------------------------------------------------------------
//  ChildSpec
//------------------------------------------------------------------------------------------------

/// A `ChildSpec` (read child-specification) defines exactly how a child may be spawned and subsequently
/// restarted. The child-specification can be added to a supervisor in order to automatically
/// spawn and supervise the process.
///
/// A new `ChildSpec` may be created using the [ChildSpec::new] function for complete control
/// over its restart behaviour. In most cases however, a `ChildSpec` can be created from a bunch
/// of different types like ...
///
/// A `ChildSpec<P, E, Fun, Fut>` can be converted into a [DynamicChildSpec] using
/// [ChildSpec::into_dyn]. This removes its type-information, but it can still be spawned and
/// supervised.
#[derive(Debug)]
pub struct ChildSpec<P, E, Fun, Fut> {
    start_fn: Fun,
    link: Link,
    phantom_data: PhantomData<(Fut, P, E)>,
}

impl<P, E, Fun, Fut> ChildSpec<P, E, Fun, Fut>
where
    P: Protocol + Send,
    E: Send + 'static,
    Fun: FnMut(StartOption<E>, Link) -> Fut + Send + 'static,
    Fut: Future<Output = RestartResult<Option<Child<E, P>>>> + Send + 'static,
{
    pub fn new(start_fn: Fun, link: Link) -> Self {
        Self {
            start_fn,
            link,
            phantom_data: PhantomData,
        }
    }

    pub fn into_dyn(self) -> DynamicChildSpec {
        DynamicChildSpec(Box::new(self))
    }

    pub async fn start(mut self) -> RestartResult<SupervisedChild<P, E, Fun, Fut>> {
        let spawn_fut = (self.start_fn)(StartOption::Start, self.link);

        match spawn_fut.await {
            Ok(Some(child)) => Ok(SupervisedChild::new(child, self.start_fn)),
            Ok(None) => panic!("May not return None when spawning for the first time"),
            Err(e) => Err(e),
        }
    }
}

impl<P, E, Fun, Fut> Clone for ChildSpec<P, E, Fun, Fut>
where
    Fun: Clone,
{
    fn clone(&self) -> Self {
        Self {
            start_fn: self.start_fn.clone(),
            link: self.link.clone(),
            phantom_data: self.phantom_data.clone(),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  DynamicChildSpec
//------------------------------------------------------------------------------------------------

/// A [ChildSpec] with its type erased.
/// This can be created using [ChildSpec::into_dyn].
pub struct DynamicChildSpec(Box<dyn Specifies + Send>);

impl DynamicChildSpec {
    pub async fn start(self) -> RestartResult<DynamicSupervisedChild> {
        self.0.box_start().await
    }
}

//------------------------------------------------------------------------------------------------
//  Specifies
//------------------------------------------------------------------------------------------------

/// A private trait used internally to create the DynamicChildSPec.
#[async_trait]
trait Specifies {
    async fn box_start(self: Box<Self>) -> RestartResult<DynamicSupervisedChild>;
    fn link(&self) -> &Link;
    fn link_mut(&mut self) -> &mut Link;
}

#[async_trait]
impl<P, E, Fun, Fut> Specifies for ChildSpec<P, E, Fun, Fut>
where
    P: Protocol + Send,
    E: Send + 'static,
    Fun: FnMut(StartOption<E>, Link) -> Fut + Send + 'static,
    Fut: Future<Output = RestartResult<Option<Child<E, P>>>> + Send + 'static,
{
    async fn box_start(mut self: Box<Self>) -> RestartResult<DynamicSupervisedChild> {
        Ok(DynamicSupervisedChild::new(self.start().await?))
    }

    fn link(&self) -> &Link {
        &self.link
    }

    fn link_mut(&mut self) -> &mut Link {
        &mut self.link
    }
}

//------------------------------------------------------------------------------------------------
//  SpawnAction
//------------------------------------------------------------------------------------------------

pub enum StartOption<E> {
    Start,
    Restart(Result<E, ExitError>),
}
