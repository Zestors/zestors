use crate::protocol;
use crate as zestors;

macro_rules! basic_actor {
    () => {
        crate::_priv::test_helper::basic_actor!(())
    };
    ($ty:ty) => {
        |mut inbox: crate::Inbox<$ty>| async move {
            loop {
                match inbox.recv().await {
                    Ok(_) => (),
                    Err(e) => break e,
                }
            }
        }
    };
}
pub(crate) use basic_actor;

macro_rules! grouped_basic_actor {
    () => {
        crate::_priv::test_helper::grouped_basic_actor!(())
    };
    ($ty:ty) => {
        |_, mut inbox: crate::Inbox<$ty>| async move {
            loop {
                match inbox.recv().await {
                    Ok(_) => (),
                    Err(e) => break e,
                }
            }
        }
    };
}
pub(crate) use grouped_basic_actor;

#[protocol]
pub(crate) enum U32Protocol {
    U32(u32),
}