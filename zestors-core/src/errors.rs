#[derive(Debug)]
pub enum TrySendDynError<M> {
    Full(M),
    Closed(M),
    NotAccepted(M),
}

#[derive(Debug)]
pub enum SendDynError<M> {
    Closed(M),
    NotAccepted(M),
}