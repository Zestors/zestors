/*!
# Inbox type
Actors can be spawned using anything that implements [`InboxType`], by default this is 
implemented for an [`inbox`] and a [`halter`]. Every inbox-type has once associated type, 
[`InboxType::Config`], which is the configuration that the inbox is spawned with. An
inbox-type must also implement [`ActorType`], which means that the [`Address<A>`] and
[`Child<A>`] will be typed with `A` equal to the inbox-type the actor was spawned with.

# Capacity
The standard configuration for inboxes that receive messages is a [`Capacity`]. This
type specifies whether the inbox is bounded or unbounded. If it is bounded then a size
is specified, and if it is unbounded then a [`BackPressure`] must be given.

# Back pressure
If the inbox is unbounded, it has a [`BackPressure`] which defines how a message-overflow
should be handled. By default the backpressure is [`expenential`](BackPressure::exponential) 
with the following parameters:
- `starts_at: 5` - The backpressure mechanism should start if the inbox contains 5 or more
messages.
- `timeout: 25ns` - The timeout at which the backpressure mechanism starts is 25ns.
- `factor: 1.3` - For every message in the inbox, the timeout is multiplied by 1.3.

The backpressure can also be set to [`linear`](BackPressure::linear) or 
[`disabled`](BackPressure::disabled).

| __<--__ [`spawning`] | [`supervision`] __-->__ |
|---|---|
 */

#[allow(unused)]
use crate::*;

#[doc(inline)]
pub use zestors_core::inboxes::*;

#[doc(inline)]
pub use {halter::Halter, inbox::Inbox};

pub mod halter {
    #[doc(inline)]
    pub use zestors_extra::halter::*;
}

pub mod inbox {
    #[doc(inline)]
    pub use zestors_extra::inbox::*;
}
