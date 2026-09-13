use super::*;
use type_sets::{AsTypeSet, Members, Subset};

pub trait IntoDyn: ActorRef + Sized {
    type Ref<T: Context>;

    fn into_context_unchecked<C>(self) -> Self::Ref<C>
    where
        C: Context;

    fn into_dyn<S>(self) -> Self::Ref<Dyn<S>>
    where
        S: Subset<<Self::Ctx as Context>::Set> + AsTypeSet + 'static,
    {
        self.into_context_unchecked()
    }

    fn into_dyn_checked<S>(self) -> Result<Self::Ref<Dyn<S>>, Self>
    where
        S: AsTypeSet + 'static + Members,
    {
        if self.is_superset_of(S::members()) {
            Ok(self.into_context_unchecked())
        } else {
            Err(self)
        }
    }

    fn downcast<I>(self) -> Result<Self::Ref<I>, Self>
    where
        I: Interface,
    {
        if self.is_interface::<I>() {
            Ok(self.into_context_unchecked())
        } else {
            Err(self)
        }
    }
}

pub trait AsDyn: IntoDyn {
    fn as_context_unchecked<S>(&self) -> &Self::Ref<S>
    where
        S: Context;

    fn as_dyn<S>(&self) -> &Self::Ref<Dyn<S>>
    where
        S: Subset<<Self::Ctx as Context>::Set> + AsTypeSet + 'static,
    {
        self.as_context_unchecked()
    }

    fn as_dyn_checked<S>(&self) -> Option<&Self::Ref<Dyn<S>>>
    where
        S: AsTypeSet + 'static + Members,
    {
        if self.is_superset_of(S::members()) {
            Some(self.as_context_unchecked())
        } else {
            None
        }
    }

    fn downcast_ref<I>(&self) -> Option<&Self::Ref<I>>
    where
        I: Interface,
    {
        if self.is_interface::<I>() {
            Some(self.as_context_unchecked())
        } else {
            None
        }
    }
}
