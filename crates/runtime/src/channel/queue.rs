use type_sets::Members;

use super::*;
use std::any::{Any, TypeId};

pub(crate) trait Queue: Any + Send + Sync + 'static {
    fn len(&self) -> usize;

    fn push_envelope_dyn(&self, msg: AnyEnvelope) -> Result<(), NotAccepted<AnyEnvelope>>;

    fn pop_dyn(&self) -> Result<AnyEnvelope, PopError>;

    fn members(&self) -> &'static [TypeId];
}

impl<I: Interface> Queue for ConcurrentQueue<I> {
    fn len(&self) -> usize {
        self.len()
    }

    fn push_envelope_dyn(&self, msg: AnyEnvelope) -> Result<(), NotAccepted<AnyEnvelope>> {
        let envelope = I::try_from_dyn_envelope(msg).map_err(|envelope| NotAccepted(envelope))?;

        self.push(envelope).map_err(|e| match e {
            PushError::Closed(_) => unreachable!("Should never be closed"),
            PushError::Full(_) => {
                panic!("Queue is full: {:?}", std::any::type_name::<Self>());
            }
        })
    }

    fn pop_dyn(&self) -> Result<AnyEnvelope, PopError> {
        self.pop().map(|interface| interface.into_dyn_envelope())
    }

    fn members(&self) -> &'static [TypeId] {
        <I::Set as Members>::members()
    }
}

impl dyn Queue {
    pub(super) fn try_push_msg<M: Message>(
        &self,
        msg: M,
    ) -> Result<M::Receipt, NotAccepted<M>> {
        let (envelope, receipt) = AnyEnvelope::new_pair::<M>(msg);

        self.push_envelope_dyn(envelope)
            .map_err(|NotAccepted(envelope)| {
                NotAccepted(
                    envelope
                        .downcast::<M>()
                        .expect("Should be the same type")
                        .msg,
                )
            })?;

        Ok(receipt)
    }
}
