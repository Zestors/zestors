use zestors_codegen::{Interface, Message};
use zestors_interface::Envelope;
use zestors_runtime::errors::DuplicatePidError;

use super::*;

#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "Result<(), DuplicatePidError>")]
pub struct RegisterChild(pub ChildSpec);

#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "Option<ChildSpec>")]
pub struct DeregisterChild(pub Pid);

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
pub enum SupervisorInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
}
