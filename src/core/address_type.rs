use crate::core::*;

/// The type of an address. Can be either `Local`, `Distr`.
///
/// This type contains a bunch of generic associated types, used to be able to create single
/// function for both local and remote addresses, even though their signatures can differ a lot.
pub trait AddrType: 'static + Send {
    /// What address does this address use internally.
    type Addr<A: 'static>: Addressable<A, AddrType = Self>;

    /// What action can be sent to this address.
    type Action<A>;

    /// The result when calling this address.
    type CallResult<M, R: RcvPart>;

    /// The result when sending an action to this address.
    type SendResult<A>;

    /// The message type which can be sent.
    /// This is LocalMsg<M> for local addresses and RemoteMsg<M> for remote addresses.
    type Msg<M>;
}

/// Any value sent in a message must implement ParamType.
/// This trait just contains a single associated type `Param`.
pub trait ParamType<'a, AT: AddrType>: Sized {
    /// This value is the value that is actually entered when sending a message to address of type AT
    type Param;
}

/// A convenience type for `<AT as AddrType>::Msg<M>`;
pub type MsgT<AT, M> = <AT as AddrType>::Msg<M>;
/// A convenience type for `<AT as AddrType>::Addr<A>`;
pub type AddrT<AT, A> = <AT as AddrType>::Addr<A>;
/// A convenience type for `<AT as AddrType>::CallResult<M, R>`;
pub type CallResultT<AT, M, R> = <AT as AddrType>::CallResult<M, R>;
/// A convenience type for `<PT as ParamType<'z, AT>>::Param`;
pub type ParamT<'z, PT, AT> = <PT as ParamType<'z, AT>>::Param;
/// A convenience type for `<AT as AddrType>::Action<A>`;
pub type ActionT<AT, A> = <AT as AddrType>::Action<A>;
/// A convenience type for `<AT as AddrType>::SendResult<A>`;
pub type SendResultT<AT, A> = <AT as AddrType>::SendResult<A>;