<div align="center">
  <p>

[![Crates.io version shield](https://img.shields.io/crates/v/zestors.svg)](https://crates.io/crates/zestors)
[![Docs](https://docs.rs/zestors/badge.svg)](https://docs.rs/zestors/0.0.1/)
[![Crates.io license shield](https://img.shields.io/crates/l/zestors.svg)](https://crates.io/crates/zestors)

 </p>
</div>

## Zestors
A simple, fast and flexible actor framework for building robust distributed applications,
heavily insipired by Erlang.

## Note
This is currently in a very early stage of development, while almost everything is
thoroughly documented, not a lot of testing has been done. Therefore, do not use this in
production yet!

Currently, `Zestors` uses nightly for the following features: `try_trait_v2` and
`associated_type_defaults`. Once those features hit stable, `Zestors` can also run on stable.

If you encounter any bugs or things that seem strange, don't hesitate to open an issue for it!

## Usage
`Zestors` revolves mainly around one single trait: `Actor`. This trait should be
implemented for any process you would like to convert to an actor. While this trait has a lot
of associated items, only the methods `init` and `handle_exit` have to be implemented.

When an `Actor` is spawned, you will get back a `Address` and a
`Process`. Addresses can be freely cloned, while there can only ever be a single
process. Both processes and addresses can be used to send messages to an `actor::Actor`.

Sending messages can be done through associated methods `.msg()`, `.msg_async()`, `.req()`, and
`.req_async()`. Functions passed to these functions will be executed on the actor. Functions
to `msg` should return a `MsgFlow`, while `req` functions should return a
`ReqFlow`.

`Process`es can be used to create supervision trees. A process can be
`hard_abort`ed or `soft_abort`ed in order to make them
exit. If a process is dropped, then the destructor will first try to soft_abort, if this fails
after the timeout set by `ABORT_TIMER`, then it will hard_abort it instead. If
you would like to disable this behaviour, then it is possible to `detatch` a
process. It can be reattatched with `re_attatch`.

All public structs/traits/functions are documented, so for more information you can take a look
at those docs. It would probably also be a good idea to look at the `Actor` trait 
definition.

## Simple example
```rust
use zestors::{
    actor::{self, Actor, ExitReason},
    flows::{InitFlow, ExitFlow, MsgFlow, ReqFlow},
    process::{ProcessExit},
    derive::{Address}
};

// What we will do first is just define our regular struct, with methods etc. Just as you normally
// would. (just adding a `derive(address)`, which is not necessary but is explained later).
//
// For this example we will use a (very) simple calculator:

#[derive(Address)] // optionally also derive (Addressable)
#[address(CalculatorAddress)] // (optional) name for generated address, this the default.
struct Calculator {
    count: i64,
}

impl Calculator {
    pub fn add(&mut self, amount: u32) {
        self.count += amount as i64;
    }

    pub fn subtract(&mut self, amount: u32) {
        self.count -= amount as i64;
    }

    pub fn result(&mut self) -> i64 {
        self.count
    }

    pub fn new(count: i64) -> Self {
        Self { count }
    }
}

// Next, let's implement a basic actor trait for our calculator

impl Actor for Calculator {
    // We override the default, which is `Self`. This value is the value that will be passed
    // as an argument to `spawn::<Calculator>(init)`.
    type Init = i64;

    // We override the default, which is `Address<Self>`. The `CalculatorAddress` is generated
    // by the `derive(Address)` macro. It is not necessary to implement this, however as you
    // will see, it is useful because we can now implement our own methods on this.
    type Address = CalculatorAddress;

    // This function should always be implemented. Since `Self::Init` is now `i64`, we initialize
    // the actor with a `Calculator::new(init)`.
    fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self> {
        InitFlow::Init(Calculator::new(init))
    }

    // This function should always be implemented.
    // Since the default `Actor::Exit` is `ExitReason<Self>`, we can just directly pass reason
    // and continue the exit, no matter the `ExitReason.
    fn handle_exit(
        self,
        state: &mut Self::State,
        reason: ExitReason<Self>,
    ) -> ExitFlow<Self> {
        ExitFlow::ContinueExit(reason)
    }
}

// That's it for our actor! we can now start creating methods for it.
// Since our `CalculatorAddress` is just a `CalculatorAddress(Address<Calculator>)`,
// we can send msgs/replies with self.0.msg(), self.0.req(). (or async versions)

impl CalculatorAddress {
    pub fn add(&self, amount: u32) {
        self.0.msg(amount, |calculator, _state, amount| {
            calculator.add(amount);
            MsgFlow::Ok
        }).send().unwrap();
    }

    pub fn subtract(&self, amount: u32) {
        self.0.msg(amount, |calculator, _state, amount| {
            calculator.subtract(amount);
            MsgFlow::Ok
        }).send().unwrap();
    }

    pub async fn result(&self) -> i64 {
        self.0.req((), |calculator, _state, ()| {
            let result = calculator.result();
            ReqFlow::Reply(result)
        }).send().unwrap().await.unwrap()
    }
}

// We're all done with out simple actor. We can now use it!

#[tokio::main]
pub async fn main() {
    // First we spawn the actor, this returns a `Process<Calculator`, and a
    // `CalculatorAddress`.
    let (mut process, address) = actor::spawn::<Calculator>(10);

    // Call some functions on it
    address.add(1);
    address.subtract(2);

    // Should now be equal to 9
    assert_eq!(address.result().await, 9);

    // Next let's soft_abort the process. This should call `handle_exit()`, which
    // currently just passes on the `ExitReason`.
    process.soft_abort();

    // And lets await the process.
    match process.await {
        ProcessExit::Handled(exit) => match exit {
                ExitReason::SoftAbort => println!("Returned with soft abort"),
                ExitReason::Error(_) => panic!(),
                ExitReason::Normal(_) => panic!(),
            },
        ProcessExit::InitFailed => panic!(),
        ProcessExit::Panic(_) => panic!(),
        ProcessExit::HardAbort => panic!(),
    }
}
```
