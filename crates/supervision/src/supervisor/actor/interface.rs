use zestors_codegen::{Interface, Message};
use zestors_runtime::channel::errors::DuplicatePidError;

use super::*;

#[derive(Message, Debug)]
#[msg(path = crate, reply = "Result<(), DuplicatePidError>")]
pub struct RegisterChild(pub ChildSpec);

#[derive(Message, Debug)]
#[msg(path = crate, reply = "Option<ChildSpec>")]
pub struct DeregisterChild(pub Pid);

#[derive(Interface, Debug)]
#[interface(path = "crate")]
pub enum SupervisorInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
}
