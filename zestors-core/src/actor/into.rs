use crate::*;

//------------------------------------------------------------------------------------------------
//  IntoChild
//------------------------------------------------------------------------------------------------

pub trait IntoChild<E, T, R>
where
    E: Send + 'static,
    T: DefinesChannel,
    R: DefinesPool,
{
    fn into_child(self) -> Child<E, T, R>;
}

impl<E, P, T2> IntoChild<E, T2, NoPool> for Child<E, P>
where
    E: Send + 'static,
    P: Protocol + TransformInto<T2>,
    T2: DefinesDynChannel,
{
    fn into_child(self) -> Child<E, T2> {
        self.transform_into()
    }
}

impl<E, P, T2> IntoChild<E, T2, Pool> for Child<E, P>
where
    E: Send + 'static,
    P: Protocol + TransformInto<T2>,
    T2: DefinesDynChannel,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.into_pool().transform_into()
    }
}

impl<E, D, T2> IntoChild<E, T2, NoPool> for Child<E, Dyn<D>>
where
    E: Send + 'static,
    T2: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T2>,
{
    fn into_child(self) -> Child<E, T2> {
        self.transform_into()
    }
}

impl<E, D, T2> IntoChild<E, T2, Pool> for Child<E, Dyn<D>>
where
    E: Send + 'static,
    T2: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T2>,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.into_pool().transform_into()
    }
}

impl<E, D, T2> IntoChild<E, T2, Pool> for ChildPool<E, Dyn<D>>
where
    E: Send + 'static,
    T2: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T2>,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.transform_into()
    }
}

impl<E, P, T2> IntoChild<E, T2, Pool> for ChildPool<E, P>
where
    E: Send + 'static,
    P: Protocol + TransformInto<T2>,
    T2: DefinesDynChannel,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.transform_into()
    }
}

//------------------------------------------------------------------------------------------------
//  IntoAddress
//------------------------------------------------------------------------------------------------

pub trait IntoAddress<T>
where
    T: DefinesChannel,
{
    fn into_address(self) -> Address<T>;
}

impl<P, T> IntoAddress<T> for Address<P>
where
    P: Protocol + TransformInto<T>,
    T: DefinesDynChannel,
{
    fn into_address(self) -> Address<T> {
        self.into_dyn()
    }
}

impl<T, D> IntoAddress<T> for Address<Dyn<D>>
where
    T: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T>,
{
    fn into_address(self) -> Address<T> {
        self.transform()
    }
}
