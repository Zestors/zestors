// use crate::function::{MsgFn, ReqFn};

// pub trait Msg: Req {
//     type MsgParams;
// }

// pub trait Req {
//     type ReqParams;
//     type Returns;
// }

// pub trait HandleMsg<M: Msg>: HandleReq<M> {
//     fn handle() -> MsgFn<Self, M::MsgParams>;
// }

// pub trait HandleReq<R: Req> {
//     fn handle() -> ReqFn<'static, Self, R::ReqParams, R::Returns>;
// }

// pub trait Handles<M>: HandleMsg<M>
// where
//     M: Msg,
// {
// }

// impl<T, M> Handles<M> for T
// where
//     T: HandleMsg<M>,
//     M: Msg,
// {
// }

// pub trait Handles2<M1, M2>: HandleMsg<M1> + HandleMsg<M2>
// where
//     M1: Msg,
//     M2: Msg
// {
// }

// impl<T, M1, M2> Handles2<M1, M2> for T
// where
//     T: HandleMsg<M1> + HandleMsg<M2>,
//     M1: Msg,
//     M2: Msg
// {
// }
