use crate::all::*;

/// Runs the event-loop of the handler.
pub(crate) async fn event_loop<H: Handler>(mut handler: H, mut state: H::State) -> H::Exit {
    let mut dead_events_in_a_row = 0;

    loop {
        let handler_item = state.next_handler_item().await;

        if let HandlerItem::Event(Event::Dead) = &handler_item {
            if dead_events_in_a_row > 5 {
                panic!("Process should exit when receiving an `Event::Dead`!")
            }
            dead_events_in_a_row += 1;
        } else {
            dead_events_in_a_row = 0;
        }

        let handler_res = handler_item.handle_with(&mut handler, &mut state).await;

        let exit_res = match handler_res {
            Ok(Flow::Continue) => ExitFlow::Continue(handler),
            Ok(Flow::ExitDirectly(exit)) => ExitFlow::Exit(exit),
            Ok(Flow::Stop(stop)) => handler.handle_exit(&mut state, Ok(stop)).await,
            Err(exception) => handler.handle_exit(&mut state, Err(exception)).await,
        };

        handler = match exit_res {
            ExitFlow::Exit(exit) => break exit,
            ExitFlow::Continue(handler) => handler,
        }
    }
}
