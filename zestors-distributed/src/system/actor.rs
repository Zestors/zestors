use super::*;
use crate::*;
use std::{io, net::SocketAddr, sync::Arc};
use tokio::net::{TcpListener, TcpStream};
use zestors::{error::RecvError, process::Inbox};
use zestors_codegen::protocol;
use zestors_core as zestors;

pub(super) struct System {
    inbox: Inbox<SystemMsg>,
    listener: TcpListener,
    inner_system: Arc<SharedSystem>,
}

impl System {
    pub fn new(
        inbox: Inbox<SystemMsg>,
        listener: TcpListener,
        inner_system: Arc<SharedSystem>,
    ) -> Self {
        System {
            inbox,
            listener,
            inner_system,
        }
    }

    pub async fn run(mut self) -> SystemExit {
        loop {
            let res = tokio::select! {
                conn = self.listener.accept() => {
                    self.handle_conn(conn).await
                }

                msg = self.inbox.recv() =>  {
                    self.handle_msg(msg).await
                }
            };

            if let Some(exit) = res {
                break (exit);
            }
        }
    }

    async fn handle_conn(
        &mut self,
        incoming: Result<(TcpStream, SocketAddr), io::Error>,
    ) -> Option<SystemExit> {
        match incoming {
            Ok((stream, socket)) => todo!(),
            Err(e) => Some(SystemExit::IoError(e)),
        }
    }

    async fn handle_msg(&mut self, msg: Result<SystemMsg, RecvError>) -> Option<SystemExit> {
        match msg {
            Ok(msg) => match msg {},
            Err(e) => match e {
                RecvError::Halted => Some(SystemExit::Halted),
                RecvError::ClosedAndEmpty => Some(SystemExit::Halted),
            },
        }
    }
}

#[protocol]
#[derive(Debug)]
pub(super) enum SystemMsg {}
