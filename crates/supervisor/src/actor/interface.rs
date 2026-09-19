use super::*;
use crate::messages::{DeregisterChild, RegisterChild};
use zestors_interface::Envelope;
use zestors_supervision::messages::{GetChildren, GetHealth};

/// The message interface a [`Supervisor`] accepts: the child/health queries
/// from `zestors-supervision` ([`GetChildren`], [`GetHealth`]), plus runtime
/// child registration ([`RegisterChild`](crate::messages::RegisterChild)) and
/// deregistration ([`DeregisterChild`](crate::messages::DeregisterChild)).
#[derive(Interface, Debug)]
#[zestors(interface_path = "zestors_interface")]
#[non_exhaustive]
pub enum SupervisorInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
    Register(Envelope<RegisterChild>),
    Deregister(Envelope<DeregisterChild>),
}
