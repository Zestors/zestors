use super::*;

#[derive(Message, Debug)]
#[msg(path = "zestors_messaging")]
pub(crate) struct Shutdown;

#[derive(Message, Debug)]
#[msg(path = "zestors_messaging")]
pub(crate) struct Suspend;

#[derive(Message, Debug)]
#[msg(path = "zestors_messaging")]
pub(crate) struct Resume;

#[derive(Message, Debug)]
#[msg(path = "zestors_messaging")]
#[msg(reply = ())]
pub(crate) struct Ping;

#[derive(Interface, Debug)]
#[interface(path = "zestors_messaging")]
pub(crate) enum SignalInterface {
    Shutdown(Envelope<Shutdown>),
    Suspend(Envelope<Suspend>),
    Resume(Envelope<Resume>),
    Ping(Envelope<Ping>),
}
