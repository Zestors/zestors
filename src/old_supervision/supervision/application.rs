use std::{fmt::Debug, time::Duration};

pub use tiny_actor::set_default_abort_timer;
use tokio::sync::OnceCell;

use crate::*;

static SYSTEM: OnceCell<Child<()>> = OnceCell::const_new();

pub struct SystemBuilder {
    abort_timer: Duration,
    key_set_fn: Option<fn()>,
    superviser_builder: SupervisorBuilder,
}

impl SystemBuilder {
    pub fn new() -> Self {
        Self {
            abort_timer: Duration::from_secs(1),
            key_set_fn: None,
            superviser_builder: SupervisorBuilder::new(
                SupervisionStrategy::OneForOne {
                    max: 10,
                    within: Duration::from_secs(10),
                },
                ShutdownStrategy::HaltThenAbort(Duration::from_secs(10)),
                RestartStrategy::Always,
            ),
        }
    }

    pub fn set_key<K: TryFrom<Key> + Debug>(mut self) -> Self {
        self.key_set_fn = Some(set_key::<K>);
        self
    }

    pub fn set_abort_timer(mut self, timer: Duration) -> Self {
        self.abort_timer = timer;
        self
    }

    pub fn add_child(self, child_spec: impl SpecifiesChild + Send + 'static) -> Self {
        Self {
            abort_timer: self.abort_timer,
            key_set_fn: self.key_set_fn,
            superviser_builder: self.superviser_builder.add_child(child_spec),
        }
    }

    pub fn spawn(self) {
        set_default_abort_timer(self.abort_timer);
        if let Some(set_key) = self.key_set_fn {
            set_key()
        }
        let (child, reff) = self.superviser_builder.spawn();
        SYSTEM.set(child).expect("Can only set the system once");
    }
}
