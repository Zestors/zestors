//! A supervision tree run as a program, with the HTTP API server attached.
//!
//! The root supervisor starts an `ApiServer` on `127.0.0.1:8080` and an app
//! supervisor with nested supervisors, `Handler` actors, closure actors and a
//! task. A background loop keeps adding short-lived tasks to one supervisor
//! through an `InMemorySupervisorSource`.
//!
//! Run it with `cargo run -p zestors --example supervision`. While it runs,
//! `just inspector run` shows the tree, or query the API directly with
//! `curl localhost:8080/processes`. Press Ctrl+C to shut the tree down
//! gracefully. The actors deliberately sleep in `init` and `exit`, so starting
//! and stopping take a few seconds.
use rootcause::Report;
use std::{sync::Arc, time::Duration};
use zestors::{
    actor::{
        BasicScheduler, Handle, Handler, HandlerContext, HandlerExit, RestartMode, fn_actor,
        fn_blueprint, fn_task,
    },
    api_server::ApiServer,
    prelude::*,
    runtime::errors::Cancelled,
    supervisor::{InMemorySupervisorSource, Node, Supervisor},
};

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

    async fn init(&mut self, _ctx: HandlerContext<'_, MyActor>) -> Result<(), Report> {
        tokio::time::sleep(Duration::from_secs(3)).await;

        self.scheduler.schedule_msg(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;

            Ok(Tick)
        });

        Ok(())
    }

    async fn exit(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
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
        _ctx: HandlerContext<'_, Self>,
        _msg: Tick,
        _req: (),
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
        _ctx: HandlerContext<'_, Self>,
        msg: u32,
        _req: (),
    ) -> Result<(), Report> {
        println!("Received message: {:?}", msg);
        Ok(())
    }
}

impl Handle<String> for MyActor {
    async fn handle(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
        msg: String,
        _req: (),
    ) -> Result<(), Report> {
        println!("Received message: {:?}", msg);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Report> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // ANCHOR: tree
    let source = InMemorySupervisorSource::new_arc();

    let (spec_a, _addr) = fn_blueprint(|| MyActor::new("A"))
        .name("HelloActor")?
        .with_mode(RestartMode::Never)
        .split();

    let (spec_b, _addr) = fn_blueprint(|| MyActor::new("B"))
        .name("HelloActor2")?
        .with_mode(RestartMode::Always)
        .split();

    let (super_spec_a, _addr) = Supervisor::blueprint()
        .children([spec_a, spec_b])
        .name("SupervisorA")?
        .split();

    let (spec_c, _addr) = fn_blueprint(|| MyActor::new("C"))
        .name("HelloActor3")?
        .with_mode(RestartMode::Always)
        .split();

    let (spec_d, _addr) = fn_blueprint(|| MyActor::new("D"))
        .name("HelloActor4")?
        .with_mode(RestartMode::Always)
        .split();

    let (super_spec_b, _addr) = Supervisor::blueprint()
        .children([spec_c, spec_d])
        .source(source.clone())
        .name("SupervisorB")?
        .split();

    let (dyn_actor_spec, _addr) = fn_actor(async |_: Inbox<MyInterface>| Ok(()))
        .name("DynActor")?
        .split();

    let (task_spec, _addr) = fn_task(|mut task_box| async move {
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
    .name("TaskActor")?
    .split();

    let app_supervisor = Supervisor::blueprint()
        .strategy(SupervisionStrategy::OneForOne)
        .children([
            super_spec_a,
            super_spec_b,
            dyn_actor_spec,
            task_spec,
            fn_blueprint(|| fn_actor(async |_: Inbox<MyInterface>| Ok(())))
                .name("DynBlueprintActor")?
                .into(),
            fn_blueprint(|| MyActor::new("E"))
                .name("DynBlueprintActor2")?
                .into(),
        ])
        .name("app-supervisor")?;

    let node = Node::new(
        Supervisor::blueprint()
            .strategy(SupervisionStrategy::RestForOne)
            .child(
                ApiServer::blueprint("127.0.0.1:8080".parse().unwrap(), "root-supervisor")
                    .name("ApiServer")?,
            )
            .child(app_supervisor)
            .name("root-supervisor")?,
    );
    // ANCHOR_END: tree

    // ANCHOR: run
    let root_address = node.root_supervisor().address().clone();
    let node_task = tokio::spawn(node.run());

    // A supervisor is running once all of its children have initialized.
    root_address.monitor_init().await?;
    tracing::info!("All actors started");

    spawn_tasks_in_background(source);

    // Runs until Ctrl+C/SIGTERM, or until the root supervisor exits.
    node_task.await??;
    Ok(())
    // ANCHOR_END: run
}

fn spawn_tasks_in_background(source: Arc<InMemorySupervisorSource>) {
    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            source
                .add(
                    fn_task(|mut task| async move {
                        task.run_until_shutdown(async {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            println!("Dynamic task completed");
                        })
                        .await?;
                        Ok(())
                    })
                    .rand_name()
                    .into_dyn(),
                )
                .expect("Name is unique");
        }
    });
}
