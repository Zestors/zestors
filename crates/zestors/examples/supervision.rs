use futures::future::pending;
use rootcause::Report;
use std::{sync::Arc, time::Duration};
use zestors::{
    actor::{
        BasicScheduler, Handle, Handler, HandlerExit, HandlerState, actor_fn, blueprint_fn, task_fn,
    },
    api_server::ApiServer,
    prelude::*,
    runtime::errors::Cancelled,
    supervision::{InMemorySupervisorSource, Supervisor},
};
use zestors_actor::RestartMode;
use zestors_supervision::{BlueprintSupervisionExt as _, Node};

#[derive(Interface, HandlerInterface)]
enum MyInterface {
    Add(Envelope<u32>),
    Print(Envelope<String>),
}

#[derive(Debug)]
struct MyActor {
    name: String,
    scheduler: BasicScheduler<MyActor>,
}

impl MyActor {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            scheduler: BasicScheduler::new(),
        }
    }
}

#[derive(Message)]
struct Tick;

impl Handler for MyActor {
    type Interface = MyInterface;

    async fn init(&mut self, _state: HandlerState<'_, MyActor>) -> Result<(), Report> {
        tokio::time::sleep(Duration::from_secs(3)).await;

        self.scheduler.schedule_msg(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;

            Ok(Tick)
        });

        Ok(())
    }

    async fn exit(
        &mut self,
        _state: HandlerState<'_, Self>,
        reason: HandlerExit,
    ) -> Result<(), Report> {
        tokio::time::sleep(Duration::from_secs(3)).await;
        reason.into()
    }

    async fn on_shutdown(&mut self, _address: &Address<Self::Interface>) -> Result<(), Report> {
        tracing::info!("Actor {} is shutting down", self.name);

        Ok(())
    }
}

impl Handle<Tick> for MyActor {
    async fn handle(
        &mut self,
        _state: HandlerState<'_, Self>,
        _msg: Envelope<Tick>,
    ) -> Result<(), Report> {
        tracing::info!("Actor {} received a tick", self.name);

        self.scheduler.schedule_msg(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            Ok(Tick)
        });

        Ok(())
    }
}

impl Handle<u32> for MyActor {
    async fn handle(
        &mut self,
        _state: HandlerState<'_, Self>,
        msg: Envelope<u32>,
    ) -> Result<(), Report> {
        println!("Received message: {:?}", msg);
        Ok(())
    }
}

impl Handle<String> for MyActor {
    async fn handle(
        &mut self,
        _state: HandlerState<'_, Self>,
        msg: Envelope<String>,
    ) -> Result<(), Report> {
        println!("Received message: {:?}", msg);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Report> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let source = InMemorySupervisorSource::new_arc();

    let (spec_a, _addr) = blueprint_fn(|| MyActor::new("A"))
        .pid("HelloActor")?
        .with_mode(RestartMode::Never)
        .split();

    let (spec_b, _addr) = blueprint_fn(|| MyActor::new("B"))
        .pid("HelloActor2")?
        .with_mode(RestartMode::Always)
        .split();

    let (super_spec_a, _addr) = Supervisor::blueprint()
        .children([spec_a, spec_b])
        .pid("SupervisorA")?
        .split();

    let (spec_c, _addr) = blueprint_fn(|| MyActor::new("C"))
        .pid("HelloActor3")?
        .with_mode(RestartMode::Always)
        .split();

    let (spec_d, _addr) = blueprint_fn(|| MyActor::new("D"))
        .pid("HelloActor4")?
        .with_mode(RestartMode::Always)
        .split();

    let (super_spec_b, _addr) = Supervisor::blueprint()
        .children([spec_c, spec_d])
        .source(source.clone())
        .pid("SupervisorB")?
        .split();

    let (dyn_actor_spec, _addr) = actor_fn(async |_: Inbox<MyInterface>| Ok(()))
        .pid("DynActor")?
        .split();

    let (task_spec, _addr) = task_fn(|mut task_box| async move {
        let mut completed_part1 = false;

        let res = task_box
            .run_until_shutdown(async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                println!("Task completed part 1");
                completed_part1 = true;

                tokio::time::sleep(Duration::from_secs(2)).await;
                println!("Task completed part 2");
            })
            .await;

        if let Err(Cancelled) = res {
            println!("Task was cancelled");
            if completed_part1 {
                // Cleanup part1, to reset for the next time this task is ran.
            }
            return Err(Cancelled.into());
        }

        Ok(())
    })
    .pid("TaskActor")?
    .split();

    let app_supervisor = Supervisor::blueprint()
        .strategy(SupervisionStrategy::OneForOne)
        .children([
            super_spec_a,
            super_spec_b,
            dyn_actor_spec,
            task_spec,
            blueprint_fn(|| actor_fn(async |_: Inbox<MyInterface>| Ok(())))
                .pid("DynBlueprintActor")?
                .into(),
            blueprint_fn(|| MyActor::new("E"))
                .pid("DynBlueprintActor2")?
                .into(),
        ])
        .pid("app-supervisor")?;

    let node = Node::new(
        Supervisor::blueprint()
            .strategy(SupervisionStrategy::RestForOne)
            .child(
                ApiServer::blueprint("127.0.0.1:8080".parse().unwrap())
                    .root_supervisor_pid("root-supervisor")
                    .pid("ApiServer")?,
            )
            .child(app_supervisor)
            .pid("root-supervisor")?,
    );

    let root_address = node.root_supervisor().address().clone();

    tokio::spawn(node.run()).await??;

    root_address.watch_init().await?;

    spawn_tasks_in_background(source);

    tracing::info!("All actors started, sending messages...");

    pending().await
}

fn spawn_tasks_in_background(source: Arc<InMemorySupervisorSource>) {
    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            source
                .add(
                    task_fn(|mut task| async move {
                        task.run_until_shutdown(async {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            println!("Dynamic task completed");
                        })
                        .await?;
                        Ok(())
                    })
                    .with_rand_pid()
                    .into_dyn(),
                )
                .expect("Pid is unique");
        }
    });
}
