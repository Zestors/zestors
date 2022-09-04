use async_trait::async_trait;
use futures::Future;
use zestors_core::{
    process::Inbox,
    protocol::{BoxedMessage, Message, Protocol, ProtocolMessage, Sends},
};

use crate::*;
use std::{any::TypeId, pin::Pin};

pub enum Event<A: Actor> {
    Sync(
        Box<
            dyn for<'a> FnOnce(&'a mut A, &'a mut Inbox<A::Protocol>) -> Result<Flow, A::Error>
                + Send
                + 'static,
        >,
    ),
    Async(
        Box<
            dyn for<'a> FnOnce(
                    &'a mut A,
                    &'a mut Inbox<A::Protocol>,
                )
                    -> Pin<Box<dyn Future<Output = Result<Flow, A::Error>> + Send + 'a>>
                + Send
                + 'static,
        >,
    ),
}

impl<A: Actor> Event<A> {
    pub async fn handle(
        self,
        actor: &mut A,
        inbox: &mut Inbox<A::Protocol>,
    ) -> Result<Flow, A::Error> {
        match self {
            Event::Sync(f) => f(actor, inbox),
            Event::Async(f) => f(actor, inbox).await,
        }
    }

    pub fn new_sync_closure<F>(f: F) -> Self
    where
        F: for<'a> FnOnce(&'a mut A, &'a mut Inbox<A::Protocol>) -> Result<Flow, A::Error>
            + Send
            + 'static,
    {
        Self::Sync(Box::new(f))
    }

    pub fn new_async_closure<F>(f: F) -> Self
    where
        F: for<'a> FnOnce(
                &'a mut A,
                &'a mut Inbox<A::Protocol>,
            )
                -> Pin<Box<dyn Future<Output = Result<Flow, A::Error>> + Send + 'a>>
            + Send
            + 'static,
    {
        Self::Async(Box::new(f))
    }
}

impl<A: Actor> Protocol for Event<A> {
    fn try_from_boxed(boxed: BoxedMessage) -> Result<Self, BoxedMessage>
    where
        Self: Sized,
    {
        boxed.downcast::<Event<A>>()
    }

    fn into_boxed(self) -> BoxedMessage {
        BoxedMessage::new::<Self>(self)
    }

    fn accepts(id: &TypeId) -> bool
    where
        Self: Sized,
    {
        *id == TypeId::of::<Self>()
    }
}

impl<A: Actor> Message for Event<A> {
    type Type = ();
}

impl<A: Actor> ProtocolMessage<Event<A>> for Event<A> {
    fn from_sends(msg: Sends<Event<A>>) -> Self
    where
        Self: Sized,
    {
        msg
    }

    fn try_into_sends(self) -> Result<Sends<Event<A>>, Self>
    where
        Self: Sized,
    {
        Ok(self)
    }
}

#[macro_export]
macro_rules! event {
    (async $path:path) => {
        $crate::Event::new_async_closure(
            |actor, inbox| Box::pin(async move { $path(actor, inbox).await })
        )
    };

    (move |$actor:ident $(: $actor_ty:ty)?, $inbox:ident $(: $inbox_ty:ty)?| $block:block) => {
        Event::new_sync_closure(
            move |$actor $(: $actor_ty)?, $inbox $(: $inbox_ty)?| {
                $block
            }
        )
    };

    (|$actor:ident $(: $actor_ty:ty)?, $inbox:ident $(: $inbox_ty:ty)?| $block:block) => {
        $crate::Event::new_sync_closure(
            |$actor $(: $actor_ty)?, $inbox $(: $inbox_ty)?| {
                $block
            }
        )
    };

    ($path:path) => {
        $crate::Event::new_sync_closure(
            move |actor, inbox| $path(actor, inbox)
        )
    };

    (|$actor:ident $(: $actor_ty:ty)?, $inbox:ident $(: $inbox_ty:ty)?| async move $block:block) => {
        $crate::Event::new_async_closure(
            |$actor $(: $actor_ty)?, $inbox $(: $inbox_ty)?| {
                Box::pin(async move { $block })
            }
        )
    };

    (|$actor:ident $(: $actor_ty:ty)?, $inbox:ident $(: $inbox_ty:ty)?| async $block:block) => {
        $crate::Event::new_async_closure(
            |$actor $(: $actor_ty)?, $inbox $(: $inbox_ty)?| {
                Box::pin(async { $block })
            }
        )
    };
}

#[async_trait]
impl<A> Handler for A
where
    A: Actor<Protocol = Event<A>>,
{
    async fn handle(
        &mut self,
        inbox: &mut Inbox<A::Protocol>,
        msg: A::Protocol,
    ) -> Result<Flow, A::Error> {
        msg.handle(self, inbox).await
    }
}