use super::*;
use type_sets::{AsTypeSet, Members, Subset};

pub trait IntoDyn: ActorRef + Sized {
    type Ref<T: Context>;

    fn into_context_unchecked<C>(self) -> Self::Ref<C>
    where
        C: Context;

    fn into_dyn_unchecked<S>(self) -> Self::Ref<Dyn<S>>
    where
        Dyn<S>: Context,
    {
        self.into_context_unchecked()
    }

    fn into_dyn<S>(self) -> Self::Ref<Dyn<S>>
    where
        S: Subset<<Self::Ctx as Context>::Set> + AsTypeSet + 'static,
    {
        self.into_dyn_unchecked()
    }

    fn into_dyn_checked<S>(self) -> Result<Self::Ref<Dyn<S>>, Self>
    where
        Dyn<S>: Context + Members,
    {
        if self.is_superset_of(Dyn::<S>::members()) {
            Ok(self.into_dyn_unchecked())
        } else {
            Err(self)
        }
    }

    fn downcast<I>(self) -> Result<Self::Ref<I>, Self>
    where
        I: Interface,
    {
        if self.is_interface::<I>() {
            // Ok(self.into_dyn_unchecked())
            todo!()
        } else {
            Err(self)
        }
    }
}

pub trait AsDyn: IntoDyn {
    fn as_dyn_unchecked<S>(&self) -> &Self::Ref<S>
    where
        S: Context;

    fn as_dyn<S>(&self) -> &Self::Ref<S>
    where
        S: Context + Subset<<Self::Ctx as Context>::Set>,
    {
        self.as_dyn_unchecked()
    }

    fn as_dyn_checked<S>(&self) -> Option<&Self::Ref<S>>
    where
        S: Context + Members,
    {
        if self.is_superset_of(S::members()) {
            Some(self.as_dyn_unchecked())
        } else {
            None
        }
    }

    fn downcast_ref<I>(&self) -> Option<&Self::Ref<I>>
    where
        I: Interface,
    {
        if self.is_interface::<I>() {
            Some(self.as_dyn_unchecked())
        } else {
            None
        }
    }
}
