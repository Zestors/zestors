use crate::RxError;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Channel is full")]
    Full(T),
}

impl<T> TrySendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            TrySendError::Closed(t) => t,
            TrySendError::Full(t) => t,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Channel is closed")]
pub struct SendError<T>(pub T);

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum SendCheckedError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

impl<T> SendCheckedError<T> {
    pub fn into_inner(self) -> T {
        match self {
            SendCheckedError::Closed(t) => t,
            SendCheckedError::NotAccepted(t) => t,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendCheckedError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Channel is full")]
    Full(T),

    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Message type not accepted by channel")]
pub struct NotAccepted<T>(pub T);

impl<T> TrySendCheckedError<T> {
    pub fn into_inner(self) -> T {
        match self {
            TrySendCheckedError::Closed(t) => t,
            TrySendCheckedError::Full(t) => t,
            TrySendCheckedError::NotAccepted(t) => t,
        }
    }
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(err: SendError<T>) -> Self {
        TrySendError::Closed(err.0)
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum RequestError<M> {
    #[error("The channel was closed")]
    Closed(M),

    #[error("No response was received")]
    NoResponse,
}

impl<M> From<SendError<M>> for RequestError<M> {
    fn from(err: SendError<M>) -> Self {
        RequestError::Closed(err.0)
    }
}

impl<M> From<RxError> for RequestError<M> {
    fn from(_err: RxError) -> Self {
        Self::NoResponse
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum RequestCheckedError<M> {
    #[error("The channel was closed")]
    Closed(M),

    #[error("The message type was not accepted by the channel")]
    NotAccepted(M),

    #[error("No response was received")]
    NoResponse,
}

impl<M> From<SendCheckedError<M>> for RequestCheckedError<M> {
    fn from(err: SendCheckedError<M>) -> Self {
        match err {
            SendCheckedError::Closed(m) => RequestCheckedError::Closed(m),
            SendCheckedError::NotAccepted(m) => RequestCheckedError::NotAccepted(m),
        }
    }
}

impl<M> From<RxError> for RequestCheckedError<M> {
    fn from(_err: RxError) -> Self {
        Self::NoResponse
    }
}

impl<M> From<NotAccepted<M>> for RequestCheckedError<M> {
    fn from(err: NotAccepted<M>) -> Self {
        RequestCheckedError::NotAccepted(err.0)
    }
}

impl<M> From<RequestError<M>> for RequestCheckedError<M> {
    fn from(err: RequestError<M>) -> Self {
        match err {
            RequestError::Closed(m) => RequestCheckedError::Closed(m),
            RequestError::NoResponse => RequestCheckedError::NoResponse,
        }
    }
}
