mod specification;
mod supervised;
mod supervisor;

use crate::process::Child;
use std::time::Duration;
use tiny_actor::Link;

pub use specification::*;
pub use supervised::*;
pub use supervisor::*;

async fn example_spawn_fn(
    init: StartOption<bool>,
    link: Link,
) -> Result<Option<Child<bool, ()>>, StartError> {
    match init {
        StartOption::Start => todo!(),
        StartOption::Restart(Ok(bool)) => todo!(),
        StartOption::Restart(Err(e)) => todo!(),
    }
}

async fn example_v2() {
    let spec = ChildSpec::new(example_spawn_fn, Link::Attached(Duration::from_secs(1)));
    let mut child = spec.start().await.unwrap();
    child.supervise().await;
    match child.restart().await {
        Ok(supervision) => match supervision {
            Supervision::Restarted => println!("Child restarted successfully"),
            Supervision::Finished => println!("Child finished successfully"),
        },
        Err(e) => println!("Restarting has failed: {:?}", e),
    }

    let spec = ChildSpec::new(example_spawn_fn, Link::Attached(Duration::from_secs(1))).into_dyn();
    let mut child = spec.start().await.unwrap();
    child.supervise().await;
    match child.restart().await {
        Ok(supervision) => match supervision {
            Supervision::Restarted => println!("Child restarted successfully"),
            Supervision::Finished => println!("Child finished successfully"),
        },
        Err(e) => println!("Restarting has failed"),
    }
}
