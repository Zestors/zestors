use super::*;
use crate::messages::{DeregisterChild, RegisterChild};
use zestors_interface::Envelope;
use zestors_supervision::messages::{GetChildren, GetHealth};

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
#[non_exhaustive]
pub enum SupervisorInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
    Register(Envelope<RegisterChild>),
    Deregister(Envelope<DeregisterChild>),
}
