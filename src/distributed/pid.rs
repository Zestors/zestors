use std::{any::Any, marker::PhantomData};

use crate::actor::Actor;
use uuid::Uuid;

use super::{node::Node, ProcessId};

#[derive(Debug)]
pub struct AnyPid(Box<dyn Any + Send>);

impl AnyPid {
    pub(crate) fn from_pid<A: Actor>(pid: Pid<A>) -> Self {
        Self(Box::new(pid))
    }

    pub fn downcast<A: Actor>(self) -> Result<Pid<A>, Self> {
        match self.0.downcast() {
            Ok(pid) => Ok(*pid),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub fn downcast_ref<A: Actor>(&self) -> Option<&Pid<A>> {
        self.0.downcast_ref()
    }
}

#[derive(Debug)]
pub struct Pid<A: Actor> {
    id: ProcessId,
    node: Node,
    a: PhantomData<A>,
}

impl<A: Actor> Pid<A> {
    pub(crate) fn new(node: Node) -> Self {
        Self {
            id: Uuid::new_v4(),
            node,
            a: PhantomData,
        }
    }

    pub fn id(&self) -> ProcessId {
        self.id
    }
}

impl<A: Actor> Clone for Pid<A> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            node: self.node.clone(),
            a: self.a.clone(),
        }
    }
}
