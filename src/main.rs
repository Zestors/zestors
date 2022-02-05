use std::time::Duration;

use zestors::{
    actor::{Actor, Exiting},
    address::{Address, FromAddress, RawAddress},
    callable::{Callable},
    flow::{MsgFlow, ReqFlow},
    func,
    state::{ActorState, State},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (child, address) = Calculator::new().spawn();
    let raw_address = child.raw_address().clone();

    address.send(func!(Calculator::add), 10)?;
    child.send(func!(Calculator::subtract), 5)?;

    let count = raw_address
        .send(func!(Calculator::get_count), ())?
        .recv()
        .await?;

    assert_eq!(count, 5);

    address.add(10);
    address.subtract(5);

    assert_eq!(address.get().await, 10);

    let tes = address
        .send(func!(Calculator::print_after_1_sec), 100)
        .unwrap().recv().await.unwrap();


    address.add(10);
    address.add(10);
    address.add(10);
    address.add(10);

    tokio::time::sleep(Duration::from_secs(5)).await;

    Ok(())
}

pub struct Calculator {
    count: i64,
}

impl Calculator {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn add(&mut self, amount: u64) -> MsgFlow<Self> {
        println!("adding...");
        self.count += amount as i64;
        MsgFlow::Ok
    }

    pub fn subtract(&mut self, amount: u64) -> MsgFlow<Self> {
        self.count -= amount as i64;
        MsgFlow::Ok
    }

    pub fn get_count(&mut self, _: ()) -> ReqFlow<Self, i64> {
        ReqFlow::Reply(self.count)
    }

    pub async fn sleep(&mut self, _: ()) -> MsgFlow<Self> {
        tokio::time::sleep(Duration::from_secs(3)).await;
        MsgFlow::Ok
    }

    pub fn print_after_1_sec(&mut self, state: &mut State<Self>, val: u32) -> ReqFlow<Self, ()> {
        state.schedule_and_then(
            async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                val
            },
            |calculator, _, val| {
                println!("value after 1s: {}, inner count: {}", val, calculator.count);
                MsgFlow::Ok
            },
        );

        state
            .this_address()
            .send(func!(Calculator::sleep), ())
            .unwrap();
        
        ReqFlow::Reply(())
    }
}

impl Actor for Calculator {
    type Address = CalculatorAddress;

    fn exiting(self, state: &mut Self::State, exit: Exiting<Self>) -> Self::Returns {
        ()
    }
}

#[derive(Clone)]
pub struct CalculatorAddress(Address<Calculator>);

impl FromAddress<Calculator> for CalculatorAddress {
    fn from_address(address: Address<Calculator>) -> Self {
        Self(address)
    }
}

impl RawAddress<Calculator> for CalculatorAddress {
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
            .recv()
            .await
            .unwrap()
    }
}
