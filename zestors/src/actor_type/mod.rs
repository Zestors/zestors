//! # Overview
//! The [`ActorType`] of a [`Child<_, A, _>`] or [`Address<A>`] specifies what type of actor it refers
//! to. Some of the things it specifies are the messages the actor can accept and the type of [`Channel`] used.
//! 
//! An [`ActorType`] can be specified in two different ways:
//! 1) As the [`InboxType`] the actor is spawned with. Some examples of built-in inboxes are the [`Inbox<Protocol>`]
//! and the [`Halter`]. An actor can choose what inbox it is spawned with, and new inboxes can be created in third-party
//! crates.
//! 2) As a [`DynActor<dyn T>`](struct@DynActor) where `T` is one of the [`dyn_types`], usually written with [`DynActor!`].
//! The actor is not specified by it's inbox, but instead by the messages it [`Accepts`].
//! 
//! # `DynActor!` macro
//! Writing [`DynActor`](struct@DynActor) types can become complicated, nstead of writing and remembering these types,
//! use the [`DynActor!`] macro to specify the [`ActorType`]:
//! - `DynActor!()` = `DynActor<dyn AcceptsNone>`
//! - `DynActor!(u32)` = `DynActor<dyn AcceptsOne<u32>>`
//! - `DynActor!(u32, u64)` = `DynActor<dyn AcceptsTwo<u32, u64>>`
//! - etc.
//! 
//! # Transforming actor-types
//! Any [`ActorRef`] that implements [`Transformable`] allows it's [`ActorType`] to be transformed into and
//! from a dynamic one. This allows references to different inbox-types to be transformed into ones of the same
//! actor-type.
//! 
//! [`Transformable`] is implemented for an [`Address<A>`] and [`Child<_, A, _>`], which allows them to be transformed
//! into an `Address<T>` and `Child<_, T, _>` as long as [`A: TransformInto<T>`](TransformInto).
//! Some examples with the default inbox-types:
//! - A [`Halter`] can be transformed into a `DynActor!()`.
//! - An [`Inbox<P>`] can be transformed into a `DynActor!(M1 .. Mn)` as long as `P` implements
//! [`FromPayload<M>`] for `M in [M1 .. Mn]`.
//! - A [`DynActor!(M1 .. Mn)`](DynActor!) can be transformed into a `DynActor!(T1 .. Tm)` as long
//! as [`T1 .. Tn`] ⊆ [`M1 .. Mm`].
//! 
//! For these examples transformation can be done with [`Transformable::transform_into`] with transformations
//! checked at compile-time. Transformations can also be checked at run-time (or not at all)
//! with [`Transformable::try_transform_into`] and [`Transformable::transform_unchecked_into`].
//! 
//! A [`DynActor`](struct@DynActor) can be downcast into the original [`InboxType`] with [`Transformable::downcast`].
//! 
//! All addresses that can be transformed implement [`IntoAddress`] and all children [`IntoChild`].
//! 
//! | __<--__ [`actor_reference`](crate::actor_reference) | [`spawning`](crate::spawning) __-->__ |
//! |---|---|
//! 
//! # Example
//! ```
#![doc = include_str!("../../examples/actor_type.rs")]
//! ```

#[allow(unused)]
use crate::all::*;

mod actor_id;
mod actor_type;
mod channel;
mod dyn_actor;
mod errors;
mod halter;
mod inbox;
mod multi_halter;
pub use {
    actor_id::*, actor_type::*, channel::*, dyn_actor::*, errors::*, halter::*,
    inbox::*, multi_halter::*,
};

pub mod dyn_types;
