use std::any::TypeId;
use zestors_core::{
    actor::{Addr},
    actor_type::{Accepts, IntoAddress},
    protocol::{BoxedMessage, Protocol, ProtocolMessage, Sends},
    DynAccepts, DynAddress,
};

pub(crate) struct TestProt;
impl Protocol for TestProt {
    fn try_from_boxed(_boxed: BoxedMessage) -> Result<Self, BoxedMessage> {
        todo!()
    }

    fn into_boxed(self) -> BoxedMessage {
        todo!()
    }

    fn accepts(_id: &TypeId) -> bool {
        todo!()
    }
}

impl ProtocolMessage<u32> for TestProt {
    fn from_sends(_msg: Sends<u32>) -> Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn try_into_sends(self) -> Result<Sends<u32>, Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

impl ProtocolMessage<u64> for TestProt {
    fn from_sends(_msg: Sends<u64>) -> Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn try_into_sends(self) -> Result<Sends<u64>, Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

fn test1(address: Addr<TestProt>, address2: DynAddress![u32, u64]) {
    address.try_send(10 as u32).unwrap();
    // address.try_send_remote(&(10 as u32));
    // address.try_send_remote(&(10 as u64));
    // address.try_send(10 as u128).unwrap();
    address2.try_send(10 as u32).unwrap();
    // address2.try_send(10 as u128).unwrap();
}

fn test2(addr: DynAddress![u32, u64]) {
    addr.clone().transform::<DynAccepts![u32]>();
    addr.clone().transform::<DynAccepts![u64]>();
    addr.clone().transform::<DynAccepts![]>();
    addr.clone().transform::<DynAccepts![u64, u32]>();
    // addr.clone().transform::<Dyn<dyn AcceptsTwo<(), u32>>>();
}

fn test3(a1: DynAddress![u32, u64], a2: Addr<TestProt>, a3: DynAddress![u32]) {
    test4(&a1);
    test4(&a2);
    test5(a1);
    test5(a2);
    // test4(a3);
}

fn test4(a: &Addr<impl Accepts<u32> + Accepts<u64>>) {
    a.try_send(10 as u32).unwrap();
    a.try_send(10 as u64).unwrap();
    // a.try_send(10 as u128).unwrap();
}

fn test5(a: impl IntoAddress<DynAccepts![u32, u64]>) {
    let a = a.into_address();
    a.try_send(10 as u32).unwrap();
}
