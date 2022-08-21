macro_rules! send_methods {
    ($at:ident) => {
        pub fn try_send<M>(&self, msg: M) -> Result<Returns<M>, TrySendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::try_send(&self.$at.channel_ref(), msg)
        }
        pub fn send_now<M>(&self, msg: M) -> Result<Returns<M>, TrySendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::send_now(&self.$at.channel_ref(), msg)
        }
        pub fn send_blocking<M>(&self, msg: M) -> Result<Returns<M>, SendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::send_blocking(&self.$at.channel_ref(), msg)
        }
        pub async fn send<M>(&self, msg: M) -> Result<Returns<M>, SendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::send(&self.$at.channel_ref(), msg).await
        }
    };
}

pub(crate) use send_methods;

macro_rules! unchecked_send_methods {
    ($at:ident) => {
        pub fn try_send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendDynError<M>>
        where
            M: Message + Send + 'static,
            Sends<M>: Send + 'static,
        {
            self.$at.channel_ref().try_send_unchecked(msg)
        }
        pub fn send_now_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendDynError<M>>
        where
            M: Message + Send + 'static,
            Sends<M>: Send + 'static,
        {
            self.$at.channel_ref().send_now_unchecked(msg)
        }
        pub fn send_blocking_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendDynError<M>>
        where
            M: Message + Send + 'static,
            Sends<M>: Send + 'static,
        {
            self.$at.channel_ref().send_blocking_unchecked(msg)
        }
        pub async fn send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendDynError<M>>
        where
            M: Message + Send + 'static,
            Sends<M>: Send + 'static,
        {
            self.$at.channel_ref().send_unchecked(msg).await
        }
    };
}

pub(crate) use unchecked_send_methods;

macro_rules! channel_methods {
    ($at:ident) => {
        pub fn close(&self) -> bool {
            self.$at.close()
        }
        pub fn halt_some(&self, n: u32) {
            self.$at.halt_some(n)
        }
        pub fn process_count(&self) -> usize {
            self.$at.process_count()
        }
        pub fn msg_count(&self) -> usize {
            self.$at.msg_count()
        }
        pub fn address_count(&self) -> usize {
            self.$at.address_count()
        }
        pub fn is_closed(&self) -> bool {
            self.$at.is_closed()
        }
        pub fn capacity(&self) -> &Capacity {
            self.$at.capacity()
        }
        pub fn has_exited(&self) -> bool {
            self.$at.has_exited()
        }
        pub fn actor_id(&self) -> u64 {
            self.$at.actor_id()
        }
        pub fn is_bounded(&self) -> bool {
            self.$at.is_bounded()
        }
    };
}

pub(crate) use channel_methods;

macro_rules! transform_methods {
    ($at:ident, $ty:ty) => {
        pub fn transform_unchecked<T>(self) -> $ty
        where
            T: ActorType<Type = Dynamic>,
        {
            <$ty>::from_inner(self.$at)
        }
    
        pub fn transform<T>(self) -> $ty
        where
            D: IntoDynamic<T>,
            T: ActorType<Type = Dynamic>,
        {
            self.transform_unchecked()
        }
    
        pub fn try_transform<T>(self) -> Result<$ty, Self>
        where
            T: IsDynamic,
            T: ActorType<Type = Dynamic>,
        {
            if T::message_ids().iter().all(|id| self.accepts(id)) {
                Ok(self.transform_unchecked())
            } else {
                Err(self)
            }
        }
    
        pub fn accepts(&self, id: &TypeId) -> bool {
            self.$at.channel_ref().accepts(id)
        }
    
        pub fn downcast<T>(self) -> Result<$ty, Self>
        where
            T: Protocol,
        {
            match self.$at.downcast::<T>() {
                Ok($at) => Ok(<$ty>::from_inner($at)),
                Err($at) => Err(Self { $at }),
            }
        }
    };
}

pub(crate) use transform_methods;

macro_rules! into_dyn_methods {
    ($at:ident, $ty:ty) => {
        pub fn into_dyn<T>(self) -> $ty
        where
            P: Protocol + IntoDynamic<T>,
            T: ActorType<Type = Dynamic>,
        {
            <$ty>::from_inner(self.$at.transform_channel(|c| c))
        }
    };
}

pub(crate) use into_dyn_methods;