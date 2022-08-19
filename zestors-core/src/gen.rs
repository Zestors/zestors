macro_rules! send_methods {
    () => {
    };
}

pub(crate) use send_methods;

macro_rules! dyn_send_methods {
    () => {
        // pub fn try_send_dyn<M>(
        //     &self,
        //     msg: M,
        // ) -> Result<<M::Type as MsgType<M>>::Returns, TrySendDynError<M>>
        // where
        //     C: ZestorsChannel,
        //     M: Message + Send + 'static,
        //     <M::Type as MsgType<M>>::Sends: Send + 'static,
        // {
        //     let (tx, rx) = <M::Type as MsgType>::new_pair();
        //     self.0
        //         .channel_ref()
        //         .try_send_boxed(BoxedMessage::new(msg, tx))
        //         .map_err(|e| e.downcast::<M>().unwrap())?;
        //     Ok(rx)
        // }
    };
}

pub(crate) use dyn_send_methods;