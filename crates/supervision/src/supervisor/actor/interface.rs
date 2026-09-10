use zestors_codegen::{Interface, Message};
use zestors_messaging::Envelope;
use zestors_runtime::channel::errors::DuplicatePidError;

use super::*;

#[derive(Message, Debug)]
#[msg(path = "zestors_messaging", reply = "Result<(), DuplicatePidError>")]
pub struct RegisterChild(pub ChildSpec);

#[derive(Message, Debug)]
#[msg(path = "zestors_messaging", reply = "Option<ChildSpec>")]
pub struct DeregisterChild(pub Pid);

#[derive(Interface, Debug)]
#[interface(path = "zestors_messaging")]
pub enum SupervisorInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
}
