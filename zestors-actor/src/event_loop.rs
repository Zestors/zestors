use crate::*;
use zestors_core::{error::RecvError, process::Inbox};

pub(super) async fn run<H: Handler>(init: H::Init, mut inbox: Inbox<H::Protocol>) -> H::Exit {
    let mut enabled = true;

    let mut actor = match H::init(init, &mut inbox).await {
        InitFlow::Start(actor) => actor,
        InitFlow::Exit(exit) => return exit,
    };

    loop {
        if enabled {
            tokio::select! {
                biased;

                event = NextEvent::new(&mut actor)  => {
                    actor = match handle_event(event, actor, &mut inbox, &mut enabled).await {
                        Ok(actor) => actor,
                        Err(exit) => break exit,
                    }
                }

                msg = inbox.recv() => {
                    actor = match handle_recv(msg, actor, &mut inbox).await {
                        Ok(actor) => actor,
                        Err(exit) => break exit,
                    }
                }

            }
        } else {
            actor = match handle_recv(inbox.recv().await, actor, &mut inbox).await {
                Ok(actor) => actor,
                Err(exit) => break exit,
            }
        }
    }
}

async fn handle_recv<H: Handler>(
    msg: Result<H::Protocol, RecvError>,
    actor: H,
    inbox: &mut Inbox<H::Protocol>,
) -> Result<H, H::Exit> {
    match msg {
        Ok(msg) => handle_msg(msg, actor, inbox).await,
        Err(e) => match actor.handle_exception(inbox, e.into()).await {
            ExitFlow::Cont(actor) => Ok(actor),
            ExitFlow::Exit(exit) => Err(exit),
        },
    }
}

async fn handle_event<H: Handler>(
    event: Event<H::Protocol>,
    actor: H,
    inbox: &mut Inbox<H::Protocol>,
    enabled: &mut bool,
) -> Result<H, H::Exit> {
    match event {
        Event::Disable => {
            *enabled = false;
            Ok(actor)
        }
        Event::Empty => Ok(actor),
        Event::Msg(msg) => handle_msg(msg, actor, inbox).await,
    }
}

async fn handle_msg<H: Handler>(
    msg: H::Protocol,
    mut actor: H,
    inbox: &mut Inbox<H::Protocol>,
) -> Result<H, H::Exit> {
    let exception = match actor.handle(inbox, msg).await {
        Ok(flow) => match flow {
            Flow::Cont => None,
            Flow::Exit(e) => return Err(e),
        },
        Err(e) => Some(Exception::Error(e)),
    };

    if let Some(exception) = exception {
        match actor.handle_exception(inbox, exception).await {
            ExitFlow::Cont(actor) => Ok(actor),
            ExitFlow::Exit(exit) => Err(exit),
        }
    } else {
        Ok(actor)
    }
}
