use std::{any::Any, marker::PhantomData, ptr::DynMetadata, sync::Arc};

use crate::{
    action::MsgFnType,
    actor::{Actor, ProcessId},
    address::{Address, Addressable},
    Fn,
};
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

#[derive(Debug)]
pub struct ProcessRef<A: ?Sized> {
    a: PhantomData<A>,
    id: ProcessId,
    node: Node,
}

impl<A: Actor> ProcessRef<A> {
    pub(crate) fn new(node: Node, id: ProcessId) -> Self {
        Self {
            id,
            node,
            a: PhantomData,
        }
    }

    pub fn id(&self) -> ProcessId {
        self.id
    }

    pub fn msg<P: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        function: MsgFnType<A, P>,
        params: &P,
    ) {
        self.node.send_action(RemoteAction::new_msg(function, params, self.id));
    }
}

impl<A> Clone for ProcessRef<A> {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            id: self.id.clone(),
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
pub struct AnyPid(Box<dyn Any + Send + Sync>);

impl AnyPid {
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
