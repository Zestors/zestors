use crate::*;

//------------------------------------------------------------------------------------------------
//  Child
//------------------------------------------------------------------------------------------------

pub struct Child<E: Send + 'static, T: ActorType = Accepts![]> {
    child: tiny_actor::Child<E, <T::Type as ChannelType>::Channel>,
}

//------------------------------------------------------------------------------------------------
//  Any child
//------------------------------------------------------------------------------------------------

impl<E, T> Child<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    gen::channel_methods!(child);
    gen::send_methods!(child);

    pub(crate) fn from_inner(
        child: tiny_actor::Child<E, <T::Type as ChannelType>::Channel>,
    ) -> Self {
        Self { child }
    }
}

//-------------------------------------------------
//  Static child
//-------------------------------------------------

impl<E, P> Child<E, P>
where
    E: Send + 'static,
    P: ActorType<Type = Static<P>>,
{
}

//-------------------------------------------------
//  Dynamic child
//-------------------------------------------------

impl<E, D> Child<E, D>
where
    E: Send + 'static,
    D: ActorType<Type = Dynamic>,
{
    gen::unchecked_send_methods!(child);
}
