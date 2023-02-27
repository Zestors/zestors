use zestors::{
    actor_ref::{ActorRefExt, Address},
    Accepts,
};
use zestors_codegen::{Envelope, Message};

#[derive(Message, Envelope, Debug)]
#[request(u64)]
#[envelope(SayHelloEnvelope, say_hello)]
pub struct SayHello {
    field_a: u32,
    field_b: String,
}

async fn test(address: Address<Accepts![SayHello]>) {
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
