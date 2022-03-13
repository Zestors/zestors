use std::time::Duration;

use crate::{actor::{Actor, ExitReason}, context::BasicCtx, address::Address, flows::{InitFlow, ReqFlow, MsgFlow}, messaging::Request};

#[derive(Debug)]
pub(crate) struct TestActor {
    val: u32,
    ctx: BasicCtx<Self>
}

#[async_trait::async_trait]
impl Actor for TestActor {
    type Init = u32;
    type Context = BasicCtx<Self>;
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self> {
        InitFlow::Init(Self { val: init, ctx: BasicCtx::new(address) })
    }

    async fn exit(self, reason: ExitReason<Self>) -> Self::ExitWith {
        reason
    }

    fn ctx(&mut self) -> &mut Self::Context {
        &mut self.ctx
    }
}

#[allow(dead_code)]
impl TestActor {
    pub async fn echo<'a>(&mut self, str: &'a str) -> ReqFlow<Self, &'a str> {
        ReqFlow::Reply(str)
    }
}


