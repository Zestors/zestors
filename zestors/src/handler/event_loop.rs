use super::*;
use crate::all::*;

pub(super) async fn start_with<H, I>(
    init: I,
    link: Link,
    cfg: HandlerInboxConfig<H>,
) -> Result<(Child<H::Exit, <H::State as HandlerState<H>>::InboxType>, H::Ref), H::StartError>
where
    H: HandleEvent + HandleExit + HandleInit<I>,
    I: Send + 'static,
{
    let (child, address) = spawn_with(link, cfg, |inbox| async move {
        let mut state = <H::State as HandlerState<H>>::from_inbox(inbox);

        match H::handle_init(init, &mut state).await {
            Ok(handler) => run(handler, &mut state).await,
            Err(exit) => exit,
        }
    });

    H::init(child, address).await
}

async fn run<H: Handler + HandleEvent + HandleExit>(
    mut handler: H,
    state: &mut H::State,
) -> H::Exit {
    loop {
        let handled_event = match state.next_action().await {
            Ok(action) => action.handle(&mut handler, state).await,
            Err(event) => handler.handle_event(state, event).await,
        };

        let exit_flow = match handled_event {
            Ok(flow) => match flow {
                Flow::Continue => HandlerExit::Resume(handler),
                Flow::Stop => handler.handle_exit(state, None).await,
            },
            Err(exception) => handler.handle_exit(state, Some(exception)).await,
        };

        handler = match exit_flow {
            HandlerExit::Exit(exit) => break exit,
            HandlerExit::Resume(handler) => handler,
        }
    }
}
