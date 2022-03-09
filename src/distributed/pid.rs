use std::{any::Any, marker::PhantomData, sync::Arc, ptr::DynMetadata};

use crate::{actor::Actor, address::{Address, Addressable}, action::MsgFnType, Fn};
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use super::{local_node::LocalNode, node::NodeActor, ProcessId, msg::ActorTypeId, remote_action::RemoteAction};

//------------------------------------------------------------------------------------------------
//  Pid
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Pid<A: ?Sized> {
    a: PhantomData<A>,
    id: ProcessId,
    node: NodeLocation,
}

impl<A: Actor> Pid<A> {
    pub(crate) fn new(node: NodeLocation) -> Self {
        Self {
            id: Uuid::new_v4(),
            node,
            a: PhantomData,
        }
    }

    pub fn id(&self) -> ProcessId {
        self.id
    }

    pub fn msg<P: Serialize + DeserializeOwned + Send + 'static>(&self, function: MsgFnType<A, P>, params: &P) {
        let action = RemoteAction::new_msg(function, params, self.id);
        match &self.node {
            NodeLocation::Local(_) => todo!(),
            NodeLocation::Remote(node) => {
                node.msg(Fn!(NodeActor::send_action), action).send();
            },
        }
    }
}

impl<A> Clone for Pid<A> {
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
    Remote(Address<NodeActor>),
}

//------------------------------------------------------------------------------------------------
//  AnyPid
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct AnyPid(Box<dyn Any + Send + Sync>);

impl AnyPid {
    pub(crate) fn new<A: 'static>(pid: Pid<A>) -> Self {
        Self(Box::new(pid))
    }

    pub fn downcast<A: 'static>(self) -> Result<Pid<A>, Self> {
        match self.0.downcast() {
            Ok(pid) => Ok(*pid),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub fn downcast_ref<A: Actor>(&self) -> Option<&Pid<A>> {
        self.0.downcast_ref()
    }

    // pub(crate) fn actor_type_id(&self) -> ActorTypeId {
    //     let boxed = *self.0;

    //     let metadata = DynMetadata::from(boxed);   
    //     let type_id = boxed.type_id();
    // }
}

unsafe impl<A> Send for Pid<A> {}
unsafe impl<A> Sync for Pid<A> {}
