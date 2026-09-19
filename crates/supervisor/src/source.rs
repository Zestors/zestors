use crate::_prelude::*;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;
use zestors_runtime::errors::DuplicateNameError;
use zestors_supervision::Start;

/// A [`SupervisorSource`] is a dynamic source of [`ChildSpec`]s for a
/// [`Supervisor`]. The implementation could be a database, a file, or anything
/// else that can provide a list of child specifications.
///
/// There is a built-in implementation called [`InMemorySupervisorSource`], which
/// allows you to manage child specs in-memory.
///
/// A source can be passed to a [`SupervisorBlueprint`] using the
/// [`source`](SupervisorBlueprint::source) method.
pub trait SupervisorSource: Debug + Send + Sync + 'static {
    /// Returns the next event from the source, or `None` if the source is closed.
    fn next(&self) -> BoxFuture<'_, Option<SupervisorSourceEvent>>;

    /// Returns all child specs currently in the source, without clearing the event queue.
    fn read_all(&self) -> BoxFuture<'_, Result<Vec<ChildSpec>, Report>>;

    /// Returns all child specs currently in the source, and clears the event queue.
    fn load_all(&self) -> BoxFuture<'_, Result<Vec<ChildSpec>, Report>>;

    /// Removes and returns the child spec for `name`, if the source has one.
    fn remove(&self, name: Name) -> BoxFuture<'_, Result<Option<ChildSpec>, Report>>;

    /// Adds `spec` to the source.
    fn add(&self, spec: ChildSpec) -> BoxFuture<'_, Result<(), Report>>;
}

/// A change to a [`SupervisorSource`]'s set of child specs, as returned by
/// [`SupervisorSource::next`].
#[derive(Debug, Clone)]
pub enum SupervisorSourceEvent {
    /// A child spec was added.
    Added(ChildSpec),
    /// The child spec for this [`Name`] was removed.
    Removed(Name),
}

/// An in-memory, thread-safe [`SupervisorSource`], for managing a
/// [`Supervisor`]'s child specs at runtime without implementing a custom
/// [`SupervisorSource`].
#[derive(Debug)]
pub struct InMemorySupervisorSource {
    inner: Mutex<LocalSupervisorChildren>,
    notify: Notify,
}

#[derive(Debug, Clone, Default)]
struct LocalSupervisorChildren {
    children: IndexMap<Name, ChildSpec>,
    events: VecDeque<SupervisorSourceEvent>,
}

impl InMemorySupervisorSource {
    /// Creates a new, empty source.
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
            notify: Default::default(),
        }
    }

    /// Same as [`InMemorySupervisorSource::new`], already wrapped in an
    /// [`Arc`] for passing to [`SupervisorBlueprint::source`].
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Adds `spec`, failing if its [`Name`] is already present.
    pub fn add<T: Start>(&self, spec: ChildSpec<T>) -> Result<(), DuplicateNameError> {
        let dyn_spec = spec.into_dyn();
        let name = dyn_spec.name().clone();

        let mut inner = self.inner.lock().unwrap();
        if inner.children.contains_key(&name) {
            return Err(DuplicateNameError { name });
        }

        inner.children.insert(name, dyn_spec.clone());
        inner
            .events
            .push_back(SupervisorSourceEvent::Added(dyn_spec));

        self.notify.notify_one();
        Ok(())
    }

    /// Removes and returns the child spec for `name`, if present.
    pub fn remove(&self, name: Name) -> Option<ChildSpec> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(spec) = inner.children.shift_remove(&name) {
            inner
                .events
                .push_back(SupervisorSourceEvent::Removed(name.clone()));

            self.notify.notify_one();
            Some(spec)
        } else {
            None
        }
    }

    /// Returns every child spec currently in the source.
    pub fn read_all(&self) -> Vec<ChildSpec> {
        self.inner
            .lock()
            .unwrap()
            .children
            .values()
            .cloned()
            .collect::<Vec<_>>()
    }

    /// Returns the child spec for `name`, if present.
    pub fn get(&self, name: &Name) -> Option<ChildSpec> {
        self.inner.lock().unwrap().children.get(name).cloned()
    }

    /// The number of child specs currently in the source.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().children.len()
    }

    /// Returns `true` if the source has no child specs.
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

    fn remove(&self, name: Name) -> BoxFuture<'_, Result<Option<ChildSpec>, Report>> {
        Box::pin(async move { Ok((*self).remove(name)) })
    }
}
