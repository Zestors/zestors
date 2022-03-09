use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{action::MsgFnType, actor::Actor, address::Addressable, inbox::Unbounded, Fn, function::MsgFn};

use super::{registry::Registry, ProcessId};

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteAction {
    function: usize,
    process_id: ProcessId,
    wrapper_fn: usize,
    params: Vec<u8>,
}

impl RemoteAction {
    fn __msg_wrapper__<A: Actor, P: Send + 'static + Serialize + DeserializeOwned>(
        registry: &Registry,
        process_id: ProcessId,
        function: usize,
        params: Vec<u8>,
    ) {
        if let Ok(address) = registry.address_by_id::<A>(process_id) {
            let params: P = bincode::deserialize(&params).unwrap();
            let function: MsgFnType<A, P> = unsafe { std::mem::transmute(function) };
            let _ = address.msg(MsgFn::new_sync(function), params).send();
        }
        
    }

    pub fn new_msg<'a, A: Actor, P: Send + 'static + Serialize + DeserializeOwned>(
        function: MsgFnType<'a, A, P>,
        params: &P,
        process_id: ProcessId
    ) -> Self {
        Self {
            process_id,
            function: function as usize,
            params: bincode::serialize(params).unwrap(),
            wrapper_fn: Self::__msg_wrapper__::<A, P> as usize,
        }
    }

    pub unsafe fn handle(self, registry: &Registry) {
        let wrapper_fn: fn(&Registry, ProcessId, usize, Vec<u8>) = unsafe{ std::mem::transmute(self.wrapper_fn) };
        wrapper_fn(registry, self.process_id, self.function, self.params)
    }


}
