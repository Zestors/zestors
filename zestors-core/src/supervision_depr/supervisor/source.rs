use crate::_prelude::*;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use std::{collections::VecDeque, sync::Mutex};
use tokio::sync::Notify;

/// A [`SupervisorSource`] is a dynamic source of [`ChildSpec`]s for a
/// [`Supervisor`]. The implementation could be a database, a file, or anything
/// else that can provide a list of child specifications.
///
/// There is a built-in implementation called [`InMemorySupervisorSource`], which
/// allows you to manage child specs in-memory.
///
/// A source can be passed to a [`SupervisorBlueprint`] using the
/// [`with_source`](SupervisorBlueprint::with_source) method.
pub trait SupervisorSource: Debug + Send + Sync + 'static {
    /// Returns the next event from the source, or `None` if the source is closed.
    fn next(&self) -> BoxFuture<'_, Option<SupervisorSourceEvent>>;

    /// Returns all child specs currently in the source, without clearing the event queue.
    fn read_all(&self) -> BoxFuture<'_, Result<Vec<ChildSpec>, Report>>;

    /// Returns all child specs currently in the source, and clears the event queue.
    fn load_all(&self) -> BoxFuture<'_, Result<Vec<ChildSpec>, Report>>;

    fn remove(&self, pid: Pid) -> BoxFuture<'_, Result<Option<ChildSpec>, Report>>;

    fn add(&self, spec: ChildSpec) -> BoxFuture<'_, Result<(), Report>>;
}

#[derive(Debug, Clone)]
pub enum SupervisorSourceEvent {
    Added(ChildSpec),
    Removed(Pid),
}

#[derive(Debug, Clone)]
pub struct InMemorySupervisorSource {
    inner: Arc<Mutex<LocalSupervisorChildren>>,
    notify: Arc<Notify>,
}

#[derive(Debug, Clone, Default)]
struct LocalSupervisorChildren {
    children: IndexMap<Pid, ChildSpec>,
    events: VecDeque<SupervisorSourceEvent>,
}

impl InMemorySupervisorSource {
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
            notify: Default::default(),
        }
    }

    pub fn add<T: Start>(&self, spec: ChildSpec<T>) -> Result<(), DuplicatePidError> {
        let dyn_spec = spec.into_dyn();
        let pid = dyn_spec.pid().clone();

        let mut inner = self.inner.lock().unwrap();
        if inner.children.contains_key(&pid) {
            return Err(DuplicatePidError { pid });
        }

        inner.children.insert(pid, dyn_spec.clone());
        inner
            .events
            .push_back(SupervisorSourceEvent::Added(dyn_spec));

        self.notify.notify_one();
        Ok(())
    }

    pub fn remove(&self, pid: Pid) -> Option<ChildSpec> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(spec) = inner.children.shift_remove(&pid) {
            inner
                .events
                .push_back(SupervisorSourceEvent::Removed(pid.clone()));

            self.notify.notify_one();
            Some(spec)
        } else {
            None
        }
    }

    pub fn read_all(&self) -> Vec<ChildSpec> {
        self.inner
            .lock()
            .unwrap()
            .children
            .values()
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn get(&self, pid: &Pid) -> Option<ChildSpec> {
        self.inner.lock().unwrap().children.get(pid).cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SupervisorSource for InMemorySupervisorSource {
    fn next(&self) -> BoxFuture<'_, Option<SupervisorSourceEvent>> {
        Box::pin(async move {
            loop {
                // 1. Prepare notification listener *before* checking event queue to avoid race conditions
                let notified = self.notify.notified();

                // 2. Try popping an event
                if let Some(event) = self.inner.lock().unwrap().events.pop_front() {
                    return Some(event);
                }

                // 3. Wait for new additions/removals
                notified.await;
            }
        })
    }

    fn read_all(&self) -> BoxFuture<'_, Result<Vec<ChildSpec>, Report>> {
        Box::pin(async move { Ok((*self).read_all()) })
    }

    fn load_all(&self) -> BoxFuture<'_, Result<Vec<ChildSpec>, Report>> {
        Box::pin(async move {
            let mut inner = self.inner.lock().unwrap();
            inner.events.drain(..);
            Ok(inner.children.values().cloned().collect::<Vec<_>>())
        })
    }

    fn add(&self, spec: ChildSpec) -> BoxFuture<'_, Result<(), Report>> {
        Box::pin(async move {
            (*self).add(spec)?;
            Ok(())
        })
    }

    fn remove(&self, pid: Pid) -> BoxFuture<'_, Result<Option<ChildSpec>, Report>> {
        Box::pin(async move { Ok((*self).remove(pid)) })
    }
}
