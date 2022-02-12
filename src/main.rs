use zestors::{spawn, Actor, ExitFlow, InitFlow, ReqFlow, Addressable};
use zestors::derive::{Address, Addressable};

#[tokio::main]
async fn main() {
    let (child, address) = spawn::<MyActor>(MyActor);
    let address = address;
    let val: u32 = 10;
    let val2: u32 = 10;

    let res = address
        .req(val, |_, _, mut val| {
            val += 1;
            ReqFlow::Reply(val)
        })
        .send()
        .unwrap()
        .await
        .unwrap();

    println!("{}", res);

    // let res = req!(&address, val, val2, |_a, _b| async {
    //     ReqFlow::Reply(val + val2)
    // });

    // let res = msg!(&address, val, val2, |_a, _b| {
    //     Flow::Ok
    // });

    // let res = msg!(&address, |a, _| {
    //     Flow::Ok
    // });

    // let res = msg!(&address, |_, b| {
    //     Flow::Ok
    // });

    // let res = msg!(&address, |a, b| {
    //     Flow::Ok
    // });
}

#[derive(Address, Addressable)]
struct MyActor2;

impl Actor for MyActor2 {
    type Address = MyActor2Address;

    fn init(init: Self::Init, _: &mut Self::State) -> zestors::InitFlow<Self> {
        InitFlow::Init(init)
    }

    fn handle_exit(
        self,
        _: &mut Self::State,
        reason: zestors::ExitReason<Self>,
    ) -> zestors::ExitFlow<Self> {
        ExitFlow::ContinueExit(reason)
    }
}

#[derive(Address, Addressable)]
struct MyActor;

impl Actor for MyActor {
    type Address = MyActorAddress;

    fn init(init: Self::Init, _: &mut Self::State) -> zestors::InitFlow<Self> {
        InitFlow::Init(init)
    }

    fn handle_exit(
        self,
        _: &mut Self::State,
        reason: zestors::ExitReason<Self>,
    ) -> zestors::ExitFlow<Self> {
        ExitFlow::ContinueExit(reason)
    }
}

