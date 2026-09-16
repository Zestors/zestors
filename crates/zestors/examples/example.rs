use rootcause::Report;
use std::time::Duration;
use zestors::{
    actor::{
        BasicScheduler, Handle, HandledBy, Handler, HandlerCallback, HandlerMessage, HandlerState,
    },
    prelude::*,
    supervision::{GetChildren, GetHealth, Health},
};
use zestors_actor::ActorExt;
use zestors_runtime::spawn;
#[tokio::main]
async fn main() {
    let child = spawn(async move |mut inbox: Inbox<MyInterface>| {
        while let Some(msg) = inbox.recv().await {
            match msg {
                MyInterface::Add(envelope) => {
                    println!("Received message: {:?}", envelope);
                }
                MyInterface::Print(envelope) => {
                    println!("Received message: {:?}", envelope);
                }
                MyInterface::Health(Envelope { req, .. }) => {
                    req.reply(Health::healthy()).ok();
                }
                MyInterface::Children(Envelope { req, .. }) => {
                    req.reply(vec![]).ok();
                }
                MyInterface::Rpc(envelope) => {
                    println!("Received any message. Not handleable, dropping...");
                    drop(envelope);
                }
            }
        }

        Ok(())
    });

    child.address().cast(10u32).await.unwrap();
    child.address().signal_shutdown();
    child.watch_exit().await.unwrap();

    // test().await;
}

#[derive(Debug)]
struct MyActor {
    nr: u32,
    interval: tokio::time::Interval,
    scheduler: BasicScheduler<MyActor>,
}

impl MyActor {
    fn new() -> Self {
        Self {
            nr: 0,
            interval: tokio::time::interval(Duration::from_secs(5)),
            scheduler: BasicScheduler::new(),
        }
    }
}

#[derive(Interface, HandlerInterface)]
enum MyInterface {
    Add(Envelope<u32>),
    Print(Envelope<String>),
    Health(Envelope<GetHealth>),
    Children(Envelope<GetChildren>),
    Rpc(Envelope<HandlerCallback<MyActor>>),
}

#[derive(Message)]
struct IntervalTick;

impl Handler for MyActor {
    type Interface = MyInterface;

    async fn next_event(&mut self) -> Option<Result<impl HandledBy<Self>, Report>> {
        tokio::select! {
            Some(result) = self.scheduler.next() => {
                Some(result)
            }

            _instant = self.interval.tick() => {
                Some(Ok(HandlerMessage::new(IntervalTick)))
            }
        }
    }
}

impl Handle<u32> for MyActor {
    async fn handle(
        &mut self,
        state: HandlerState<'_, Self>,
        Envelope { msg, req: () }: Envelope<u32>,
    ) -> Result<(), Report> {
        println!("Received message: {:?}", msg);

        self.nr += msg;

        if msg == 301 {
            state.signal_shutdown();
        }

        Ok(())
    }
}

impl Handle<String> for MyActor {
    async fn handle(
        &mut self,
        _: HandlerState<'_, Self>,
        msg: Envelope<String>,
    ) -> Result<(), Report> {
        println!("Received message: {:?}", msg);
        Ok(())
    }
}

impl Handle<IntervalTick> for MyActor {
    async fn handle(
        &mut self,
        _: HandlerState<'_, Self>,
        _: Envelope<IntervalTick>,
    ) -> Result<(), Report> {
        println!("Interval tick: {}", self.nr);
        Ok(())
    }
}

impl Handle<GetHealth> for MyActor {
    async fn handle(
        &mut self,
        _state: HandlerState<'_, Self>,
        Envelope {
            msg: _,
            req: handle,
        }: Envelope<GetHealth>,
    ) -> Result<(), Report> {
        handle.reply(Health::healthy().with_debug_repr(&self)).ok();

        self.scheduler.schedule_msg(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok("Hello".to_string())
        });

        self.scheduler.schedule_fut(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        });

        Ok(())
    }
}

impl Handle<GetChildren> for MyActor {
    async fn handle(
        &mut self,
        _: HandlerState<'_, Self>,
        Envelope {
            msg: _,
            req: handle,
        }: Envelope<GetChildren>,
    ) -> Result<(), Report> {
        handle.reply(vec![]).ok();
        Ok(())
    }
}

async fn test() {
    let child = MyActor::new()
        // .map_actor_exit(|x| x.map(|x| x * 2))
        .spawn();
    let address = child.address().clone();

    address.cast(5u32).await.unwrap();
    child.cast(15u32).await.unwrap();
    child.cast("Hello, world!".to_string()).await.unwrap();

    child
        .cast(HandlerCallback::new(|actor, state| Ok(())))
        .await
        .unwrap();

    child.signal_shutdown();
    child.await.unwrap();
}
