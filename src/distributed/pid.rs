use std::{any::Any, marker::PhantomData, ptr::DynMetadata, sync::Arc};

use crate::{
    action::MsgFnType,
    actor::{Actor, ProcessId},
    address::{Address, Addressable},
    Fn, function::MsgFn,
};
use derive_more::DebugCustom;
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use super::{
    local_node::LocalNode,
    msg::ActorTypeId,
    node::{Node, NodeActor},
    remote_action::RemoteAction,
};

//------------------------------------------------------------------------------------------------
//  Pid
//------------------------------------------------------------------------------------------------

#[derive(DebugCustom)]
#[debug(fmt = "ProcessRef {{ id: {}, node: {:?} }}", "process_id", "node")]
pub struct ProcessRef<A: ?Sized> {
    a: PhantomData<A>,
    process_id: ProcessId,
    node: Node,
}

impl<A: Actor> ProcessRef<A> {
    pub(crate) unsafe fn new(node: Node, id: ProcessId) -> Self {
        Self {
            process_id: id,
            node,
            a: PhantomData,
        }
    }

    pub fn id(&self) -> ProcessId {
        self.process_id
    }

    pub fn msg<P: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        function: MsgFn<A, P>,
        params: &P,
    ) {
        self.node.send_action(RemoteAction::new_msg(function, params, self.process_id));
    }
}

impl<A> Clone for ProcessRef<A> {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            process_id: self.process_id.clone(),
            node: self.node.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum NodeLocation {
    Local(LocalNode),
    Remote(Node),
}

//------------------------------------------------------------------------------------------------
//  AnyPid
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct AnyProcessRef(Box<dyn Any + Send + Sync>);

impl AnyProcessRef {
    pub(crate) fn new<A: 'static>(pid: ProcessRef<A>) -> Self {
        Self(Box::new(pid))
    }

    pub fn downcast<A: 'static>(self) -> Result<ProcessRef<A>, Self> {
        match self.0.downcast() {
            Ok(pid) => Ok(*pid),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub fn downcast_ref<A: Actor>(&self) -> Option<&ProcessRef<A>> {
        self.0.downcast_ref()
    }

    // pub(crate) fn actor_type_id(&self) -> ActorTypeId {
    //     let boxed = *self.0;

    //     let metadata = DynMetadata::from(boxed);
    //     let type_id = boxed.type_id();
    // }
}

unsafe impl<A> Send for ProcessRef<A> {}
unsafe impl<A> Sync for ProcessRef<A> {}
