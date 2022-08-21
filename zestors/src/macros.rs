#[cfg(test)]
mod test {
    use crate as zestors;
    use crate::*;
    use std::any::TypeId;

    #[test]
    fn basic_macros() {
        use msg::*;
        mod msg {
            use super::*;

            #[derive(Message, Debug, Clone)]
            #[msg(Request<()>)]
            pub struct Msg1;

            #[derive(Message)]
            pub struct Msg2;

            #[derive(Message)]
            #[msg(Request<u32>)]
            pub enum Msg3 {
                Variant,
            }

            #[derive(Message)]
            pub enum Msg4 {
                Variant,
            }
        }

        #[protocol]
        enum Prot {
            One(Msg1),
            Two(Msg2),
            Three(Msg3),
            Four(Msg4),
        }

        Prot::One((Msg1, Request::new().0));
        Prot::Two(Msg2);
        Prot::Three((Msg3::Variant, Request::new().0));
        Prot::Four(Msg4::Variant);

        <Prot as Protocol>::try_from_boxed(BoxedMessage::new::<Msg1>((Msg1, Request::new().0)))
            .unwrap();
        <Prot as Protocol>::try_from_boxed(BoxedMessage::new::<Msg2>(Msg2)).unwrap();
        <Prot as Protocol>::try_from_boxed(BoxedMessage::new::<Msg3>((
            Msg3::Variant,
            Request::new().0,
        )))
        .unwrap();
        <Prot as Protocol>::try_from_boxed(BoxedMessage::new::<Msg4>(Msg4::Variant)).unwrap();

        assert!(<Prot as Protocol>::accepts(&TypeId::of::<Msg1>()));
        assert!(<Prot as Protocol>::accepts(&TypeId::of::<Msg2>()));
        assert!(<Prot as Protocol>::accepts(&TypeId::of::<Msg3>()));
        assert!(<Prot as Protocol>::accepts(&TypeId::of::<Msg4>()));
        assert!(!<Prot as Protocol>::accepts(&TypeId::of::<u32>()));

        <Prot as ProtocolMessage<Msg1>>::from_sends((Msg1, Request::new().0));
        <Prot as ProtocolMessage<Msg2>>::from_sends(Msg2);
        <Prot as ProtocolMessage<Msg3>>::from_sends((Msg3::Variant, Request::new().0));
        <Prot as ProtocolMessage<Msg4>>::from_sends(Msg4::Variant);
    }
}
