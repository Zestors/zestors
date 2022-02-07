use std::time::Duration;

use futures::StreamExt;
use zestors::{
    actor::{spawn, Actor, ExitReason, Spawn},
    address::{Address, Addressable, FromAddress},
    callable::Callable,
    flows::{ExitFlow, IntoReplyAble, Flow, InitFlow, ReqFlow},
    func,
    messaging::{Reply, Req},
    packets::Unbounded,
    sending::{BoundedSend, UnboundedSend},
    state::{State, BasicState}, process::ProcessExit,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (mut process, address) = spawn::<Calculator>(());
    // let (child, address) = Calculator::new().spawn();
    // let raw_address = process.raw_address().clone();

    // println!("process exit: {:?}", process.await);
    // println!("done polling");
    // process.send(func!(Calculator::subtract), 5)?;

    // // println!("{:?}", process.await);

    // address.send(func!(Calculator::add), 10)?;

    // let count = raw_address.send(func!(Calculator::get_count), ())?.await?;

    // assert_eq!(count, 5);

    // address.add(10);
    // address.subtract(5);
    // println!("everything done 3!");

    // tokio::time::sleep(Duration::from_micros(1)).await;

    // let (tx1, rx1) = oneshot::channel::<()>();
    // let (tx2, mut rx2) = async_channel::unbounded::<u32>();
    // tx2.send(1).await.unwrap();
    // tx2.send(2).await.unwrap();
    // tx2.send(3).await.unwrap();
    // let next1 = rx2.next();

    // drop(next1);

    // let next2 = rx2.next();
    // // let next3 = rx2.next();
    // // let next4 = rx2.next();

    // println!("{}", next2.await.unwrap());
    // println!("{}", next3.await.unwrap());
    // println!("{}", next4.await.unwrap());

    // tx2.try_send(tx1).unwrap();
    // drop(tx2);
    // drop(rx2);

    // rx1.await.unwrap();

    // assert_eq!(address.get().await, 10);
    process.hard_abort();
    match process.await {
        ProcessExit::Panic(panic) => todo!(),
        ProcessExit::HardAbort => todo!(),
        ProcessExit::InitFailed => todo!(),
        ProcessExit::Handled(exit) => match exit {
            ExitReason::Error(e) => todo!(),
            ExitReason::Normal(_) => todo!(),
            ExitReason::SoftAbort => todo!(),
        },
    }
    // drop(process);
    // tokio::time::sleep(Duration::from_micros(1)).await;
    // println!("awaited process: {:?}", process.await);

    let tes: Reply<_> = address
        .send(func!(Calculator::print_after_1_sec), 100)
        .unwrap();

    tes.async_recv().await.unwrap();

    let tes = address
        .send(func!(Calculator::print_after_1_sec), 100)
        .unwrap()
        .await
        .unwrap();

    // println!("everything done 4!");

    // address.add(10);
    // address.add(10);
    // address.add(10);
    // address.add(10);

    // tokio::time::sleep(Duration::from_secs(5)).await;

    // println!("everything done final!");

    Ok(())
}

#[derive(Debug)]
pub struct Calculator {
    count: i64,
}

impl Calculator {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn add(&mut self, amount: u64) -> Flow<Self> {
        println!("adding...");
        self.count += amount as i64;
        Flow::Ok
    }

    pub fn subtract(&mut self, amount: u64) -> Flow<Self> {
        self.count -= amount as i64;
        Flow::Ok
    }

    pub fn get_count(&mut self, _: ()) -> ReqFlow<Self, i64> {
        ReqFlow::Reply(self.count)
    }

    pub async fn sleep(&mut self, _: ()) -> Flow<Self> {
        tokio::time::sleep(Duration::from_secs(3)).await;
        Flow::Ok
    }

    pub fn print_after_1_sec(&mut self, state: &mut BasicState<Self>, val: u32) -> ReqFlow<Self, ()> {
        state.schedule_and_then(
            async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                val
            },
            |calculator, _, val| {
                println!("value after 1s: {}, inner count: {}", val, calculator.count);
                Flow::Ok
            },
        );

        state
            .address()
            .send(func!(Calculator::sleep), ())?;

        ReqFlow::Reply(())
    }
}

impl Actor for Calculator {
    type Address = CalculatorAddress;
    type Init = ();
    type Exit = ExitReason<Self>;

    fn handle_exit(self, _state: &mut Self::State, exit: ExitReason<Self>) -> ExitFlow<Self> {
        match &exit {
            ExitReason::Error(e) => {
                println!("Exited with error: {:?}", e);
                ExitFlow::Resume(self)
            }
            ExitReason::Normal(n) => {
                println!("exited normally: {:?}", n);
                ExitFlow::ContinueExit(exit)
            },
            ExitReason::SoftAbort => {
                println!("Soft aborted");
                ExitFlow::ContinueExit(exit)
            },
        }
    }

    fn init(_init: Self::Init, _state: &mut Self::State) -> InitFlow<Self> {
        InitFlow::Init(Self::new())
    }
}

#[derive(Clone)]
pub struct CalculatorAddress(Address<Calculator>);

impl FromAddress<Calculator> for CalculatorAddress {
    fn from_address(address: Address<Calculator>) -> Self {
        Self(address)
    }
}

impl Addressable<Calculator> for CalculatorAddress {
    fn raw_address(&self) -> &Address<Calculator> {
        &self.0
    }
}

impl CalculatorAddress {
    pub fn add(&self, amount: u64) {
        self.send(func!(Calculator::add), amount).unwrap();
    }

    pub fn subtract(&self, amount: u64) {
        self.send(func!(Calculator::subtract), amount).unwrap();
    }

    pub async fn get(&self) -> i64 {
        self.send(func!(Calculator::get_count), ())
            .unwrap()
            .await
            .unwrap()
    }
}
