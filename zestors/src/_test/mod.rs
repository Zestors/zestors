use crate as zestors;
use crate::protocol;

macro_rules! basic_actor {
    () => {
        $crate::_test::basic_actor!(())
    };
    ($ty:ty) => {
        |mut inbox: crate::all::Inbox<$ty>| async move {
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

macro_rules! pooled_basic_actor {
    () => {
        $crate::_test::pooled_basic_actor!(())
    };
    ($ty:ty) => {
        |_, mut inbox: $crate::all::Inbox<$ty>| async move {
            loop {
                match inbox.recv().await {
                    Ok(_) => (),
                    Err(e) => break e,
                }
            }
        }
    };
}
pub(crate) use pooled_basic_actor;

#[protocol]
pub(crate) enum U32Protocol {
    U32(u32),
}
