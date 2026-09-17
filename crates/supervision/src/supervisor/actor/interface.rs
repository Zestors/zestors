use zestors_codegen::Interface;
use zestors_interface::Envelope;

use super::*;

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
#[non_exhaustive]
pub enum SupervisorInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
    Register(Envelope<RegisterChild>),
    Deregister(Envelope<DeregisterChild>),
}
