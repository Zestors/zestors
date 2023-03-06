use zestors::{
    actor_reference::{ActorRefExt, Address},
    DynActor,
};
use zestors_codegen::{Envelope, Message};

#[allow(unused)]
#[derive(Message, Envelope, Debug)]
#[request(u64)]
#[envelope(SayHelloEnvelope, say_hello)]
pub struct SayHello {
    field_a: u32,
    field_b: String,
}

#[allow(unused)]
async fn test(address: Address<DynActor!(SayHello)>) {
    address
        .request(SayHello {
            field_a: 10,
            field_b: String::from("hi"),
        })
        .await
        .unwrap();
    address
        .envelope(SayHello {
            field_a: 10,
            field_b: String::from("hi"),
        })
        .request()
        .await
        .unwrap();
    address
        .say_hello(10, String::from("hi"))
        .request()
        .await
        .unwrap();
}
