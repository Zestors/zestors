/*!
# Overview

A big advantage of using an actor-system is the ability to write actors in a declarative manner.
This module implements a way to write actors of this kind with the [`Handler`] trait. 

# Handler
The [`Handler`] trait specifies properties of the actor like it's [`InboxType`] and [`Handler::Exit`]. 
If a handler wishes to handle message `M` it must define this in it's [`Protocol`] and implement 
[`HandleMessage<M>`]. The [`Handler`]-trait can be derived with the [`crate::Handler!`]-macro.

# HandlerState
Every handler has to specify a [`HandlerState`]. This state can generate [`HandlerItem`]s that the
actor subsequently handles. Examples of a handler-state are the [`Inbox`] and [`Scheduler`]. The
inbox is the most basic version, which just receives messages and forwards them to the handler. The
scheduler also allows for scheduling arbitrary futures and a stream on the actor which can run
asynchronously; the scheduler automatically takes care of selecting messages from the inbox and
futures scheduled.

# Action
An [`Action`] is a closure that an actor can execute with it's [`Handler::State`]. Actions can
be created with the [`action!`] macro, see it's documentation for exact usage. When an actor
accepts the message [`Action<Self>`], then actions (closures) can be sent and handled
automatically on the actor. An upside of this is that it is now possible to send messages with
generics, but a downside is that the actor's protocol is now tightly coupled to the state. 

| __<--__ [`spawning`](crate::spawning) | [`supervision`](crate::supervision) __-->__ |
|---|---|

# Example
```
#[macro_use]
extern crate zestors;
use zestors::{export::async_trait, prelude::*};

// Let's start by defining a message ..
#[derive(Message, Debug)]
#[request(u32)]
pub struct PrintString {
    val: String,
}

// .. and a protocol.
#[protocol]
#[derive(Debug)]
pub enum MyProtocol {
    A(u32),
    B(PrintString),
    C(Action<MyHandler>),
}

// Now we can define our handler ..
#[derive(Debug)]
pub struct MyHandler {
    handled: u32,
}

// .. and implement the main trait `Handler` (or use #[derive(Handler)])
#[async_trait]
impl Handler for MyHandler {
    type State = Inbox<MyProtocol>;
    type Exception = eyre::Report;
    type Stop = ();
    type Exit = u32;

    async fn handle_exit(
        self,
        _state: &mut Self::State,
        reason: Result<Self::Stop, Self::Exception>,
    ) -> ExitFlow<Self> {
        match reason {
            // Upon ok, we exit normally.
            Ok(()) => ExitFlow::Exit(self.handled),
            // When an error occured, we also exit but log it.
            Err(exception) => {
                println!("[ERROR] Actor exited with an exception: {exception}");
                ExitFlow::Exit(self.handled)
            }
        }
    }

    async fn handle_event(&mut self, state: &mut Self::State, event: Event) -> HandlerResult<Self> {
        // For most events we stop our actor.
        // When the actor is halted we just close the inbox and continue until it is empty.
        match event {
            Event::Halted => {
                state.close();
                Ok(Flow::Continue)
            }
            Event::ClosedAndEmpty => Ok(Flow::Stop(())),
            Event::Dead => Ok(Flow::Stop(())),
        }
    }
}

// Let's handle a u32 message ..
#[async_trait]
impl HandleMessage<u32> for MyHandler {
    async fn handle_msg(
        &mut self,
        _state: &mut Self::State,
        msg: u32,
    ) -> Result<Flow<Self>, Self::Exception> {
        self.handled += msg;
        Ok(Flow::Continue)
    }
}

// .. and our custom request.
#[async_trait]
impl HandleMessage<PrintString> for MyHandler {
    async fn handle_msg(
        &mut self,
        _state: &mut Self::State,
        (msg, tx): (PrintString, Tx<u32>),
    ) -> Result<Flow<Self>, Self::Exception> {
        println!("{}", msg.val);
        let _ = tx.send(self.handled);
        self.handled += 1;
        Ok(Flow::Continue)
    }
}

// That was all!
#[tokio::main]
async fn main() {
    // Let's spawn our actor:
    let (child, address) = MyHandler { handled: 0 }.spawn();
    // We can send it a basic message:
    address.send(10u32).await.unwrap();

    // Send it a bunch of requests that will print the message.
    for i in 0..10 {
        let response = address
            .request(PrintString {
                val: String::from("Printing a message"),
            })
            .await
            .unwrap();
        println!("Got response {i} = {response}");
    }

    // Or send it a custom closure with the `action!` macro
    child
        .send(action!(|handler: &mut MyHandler, _state| async move {
            println!("This is now 20: `{}`", handler.handled);
            Ok(Flow::Continue)
        }))
        .await
        .unwrap();

    // And finally our actor can be halted and will exit!
    child.halt();
    assert!(matches!(child.await, Ok(20)));
}
```
*/

#[allow(unused)]
use crate::all::*;

mod action;
mod handler_ext;
mod state;
mod handler;
mod event_loop;
mod scheduler;
pub use {action::*, handler_ext::*, state::*, handler::*, scheduler::*};
