use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::Poll,
};

use futures::{Future, FutureExt, Stream, StreamExt};
use tokio::time::Instant;

use crate::core::*;

//------------------------------------------------------------------------------------------------
//  Inbox
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct State<A> {
    receiver: async_channel::Receiver<InternalMsg<A>>,
    rcv_signal: Option<Rcv<ChildMsg>>,
    process_id: ProcessId,
    actor_scheduler_enabled: bool,
    shared: Arc<SharedProcessData>,
}

impl<A> Stream for State<A> {
    type Item = InboxMsg<A>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if let Some(rcv_signal) = &mut self.rcv_signal {
            match rcv_signal.poll_unpin(cx) {
                Poll::Ready(signal) => {
                    self.rcv_signal.take().unwrap();
                    match signal.unwrap() {
                        ChildMsg::SoftAbort => {
                            self.receiver.close();
                            Poll::Ready(Some(InboxMsg::Signal(InboxSignal::SoftAbort)))
                        }
                        ChildMsg::Isolated => {
                            Poll::Ready(Some(InboxMsg::Signal(InboxSignal::Isolated)))
                        }
                    }
                }

                Poll::Pending => self.receiver.poll_next_unpin(cx).map(|val| {
                    val.map(|action| {
                        let InternalMsg::Action(action) = action;
                        InboxMsg::Action(action)
                    })
                }),
            }
        } else {
            self.receiver.poll_next_unpin(cx).map(|val| {
                val.map(|action| {
                    let InternalMsg::Action(action) = action;
                    InboxMsg::Action(action)
                })
            })
        }
    }
}

impl<A> State<A> {
    pub(crate) fn new(
        receiver: async_channel::Receiver<InternalMsg<A>>,
        rcv_signal: Rcv<ChildMsg>,
        process_id: ProcessId,
        shared: Arc<SharedProcessData>,
    ) -> Self {
        Self {
            actor_scheduler_enabled: true,
            process_id,
            receiver,
            rcv_signal: Some(rcv_signal),
            shared,
        }
    }

    pub(crate) fn scheduler_enabled(&self) -> bool {
        self.actor_scheduler_enabled
    }

    pub(crate) fn enable_scheduler(&mut self) {
        self.actor_scheduler_enabled = true
    }

    pub(crate) fn disable_scheduler(&mut self) {
        self.actor_scheduler_enabled = false
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.receiver.is_closed()
    }

    pub(crate) fn close(&mut self) -> bool {
        self.receiver.close()
    }

    pub(crate) fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub async fn recv(&mut self) -> Result<InboxMsg<A>, InboxRecvError> {
        self.next().await.ok_or(InboxRecvError)
    }

    pub fn try_recv(&mut self) -> Result<Option<InboxMsg<A>>, InboxRecvError> {
        if let Some(rcv_signal) = self.rcv_signal.as_mut() {
            match rcv_signal.try_recv() {
                Ok(Some(signal)) => {
                    self.rcv_signal.take().unwrap();
                    match signal {
                        ChildMsg::SoftAbort => {
                            self.receiver.close();
                            return Ok(Some(InboxMsg::Signal(InboxSignal::SoftAbort)));
                        }
                        ChildMsg::Isolated => {
                            return Ok(Some(InboxMsg::Signal(InboxSignal::Isolated)))
                        }
                    }
                }
                Ok(None) => (),
                Err(_) => unreachable!("Request never be dropped/canceled"),
            }
        }

        match self.receiver.try_recv() {
            Ok(InternalMsg::Action(action)) => Ok(Some(InboxMsg::Action(action))),
            Err(e) => match e {
                async_channel::TryRecvError::Empty => Ok(None),
                async_channel::TryRecvError::Closed => Err(InboxRecvError),
            },
        }
    }
}

impl<A> Drop for State<A> {
    fn drop(&mut self) {
        // Close the inbox.
        self.receiver.close();

        // Set exited to true.
        self.shared.exit();

        // And empty the inbox
        while self.try_recv().is_ok() {}
    }
}

pub enum InboxMsg<A> {
    Action(Action<A>),
    Signal(InboxSignal),
}

pub enum InboxSignal {
    SoftAbort,
    Isolated,
}

//------------------------------------------------------------------------------------------------
//  ActorMsg
//------------------------------------------------------------------------------------------------

pub(crate) enum InternalMsg<A> {
    Action(Action<A>),
}

//------------------------------------------------------------------------------------------------
//  ChildSignal
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub enum ChildMsg {
    SoftAbort,
    Isolated,
}

//------------------------------------------------------------------------------------------------
//  Recv Errors
//------------------------------------------------------------------------------------------------

/// There are no more `Addr`esses coupled to this `Inbox`.
pub struct InboxRecvError;

impl From<async_channel::RecvError> for InboxRecvError {
    fn from(_: async_channel::RecvError) -> Self {
        Self
    }
}
