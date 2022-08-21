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
