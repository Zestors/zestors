//! How messages between actors are laid out, on
//! [`Protocol::ACTORS`](crate::link::Protocol::ACTORS): a kind byte,
//! then the fields of that kind.

use super::RemoteError;
use crate::Id;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use zestors_runtime::Pid;

const KIND_CAST: u8 = 1;
const KIND_CALL: u8 = 2;
const KIND_REPLY: u8 = 3;

const OK: u8 = 0;
const ERR: u8 = 1;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Frame {
    /// A message that expects no reply.
    Cast {
        target: Pid,
        msg: Id,
        payload: Bytes,
    },
    /// A message that is answered by a [`Frame::Reply`] with the same `call_id`.
    Call {
        call_id: u64,
        target: Pid,
        msg: Id,
        payload: Bytes,
    },
    Reply {
        call_id: u64,
        result: Result<Bytes, RemoteError>,
    },
}

impl Frame {
    pub(super) fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        match self {
            Frame::Cast {
                target,
                msg,
                payload,
            } => {
                buf.put_u8(KIND_CAST);
                put_target(&mut buf, target, msg);
                buf.put_slice(payload);
            }
            Frame::Call {
                call_id,
                target,
                msg,
                payload,
            } => {
                buf.put_u8(KIND_CALL);
                buf.put_u64(*call_id);
                put_target(&mut buf, target, msg);
                buf.put_slice(payload);
            }
            Frame::Reply { call_id, result } => {
                buf.put_u8(KIND_REPLY);
                buf.put_u64(*call_id);
                match result {
                    Ok(payload) => {
                        buf.put_u8(OK);
                        buf.put_slice(payload);
                    }
                    Err(error) => {
                        buf.put_u8(ERR);
                        put_error(&mut buf, error);
                    }
                }
            }
        }
        buf.freeze()
    }

    pub(super) fn decode(mut bytes: Bytes) -> Option<Self> {
        match get_u8(&mut bytes)? {
            KIND_CAST => {
                let (target, msg) = get_target(&mut bytes)?;
                Some(Frame::Cast {
                    target,
                    msg,
                    payload: bytes,
                })
            }
            KIND_CALL => {
                let call_id = get_u64(&mut bytes)?;
                let (target, msg) = get_target(&mut bytes)?;
                Some(Frame::Call {
                    call_id,
                    target,
                    msg,
                    payload: bytes,
                })
            }
            KIND_REPLY => {
                let call_id = get_u64(&mut bytes)?;
                let result = match get_u8(&mut bytes)? {
                    OK => Ok(bytes),
                    ERR => Err(get_error(bytes)?),
                    _ => return None,
                };
                Some(Frame::Reply { call_id, result })
            }
            _ => None,
        }
    }
}

/// Who a message is for, and what it is: the pid (length-prefixed) and the message id.
fn put_target(buf: &mut BytesMut, target: &Pid, msg: &Id) {
    let pid = String::from(target);
    buf.put_u16(pid.len() as u16);
    buf.put_slice(pid.as_bytes());
    buf.put_u128(msg.as_uuid().as_u128());
}

fn get_target(bytes: &mut Bytes) -> Option<(Pid, Id)> {
    let len = get_u16(bytes)? as usize;
    if bytes.remaining() < len {
        return None;
    }
    let pid = std::str::from_utf8(&bytes.split_to(len)).ok()?.to_owned();
    if bytes.remaining() < 16 {
        return None;
    }
    Some((Pid::new(pid), Id::from_u128(bytes.get_u128())))
}

const E_UNKNOWN_MESSAGE: u8 = 1;
const E_NO_SUCH_ACTOR: u8 = 2;
const E_NOT_ACCEPTED: u8 = 3;
const E_CLOSED: u8 = 4;
const E_OVERLOADED: u8 = 5;
const E_NO_REPLY: u8 = 6;
const E_DECODE: u8 = 7;
const E_ENCODE: u8 = 8;
const E_TOO_LARGE: u8 = 9;

fn put_error(buf: &mut BytesMut, error: &RemoteError) {
    match error {
        RemoteError::UnknownMessage => buf.put_u8(E_UNKNOWN_MESSAGE),
        RemoteError::NoSuchActor => buf.put_u8(E_NO_SUCH_ACTOR),
        RemoteError::NotAccepted => buf.put_u8(E_NOT_ACCEPTED),
        RemoteError::Closed => buf.put_u8(E_CLOSED),
        RemoteError::Overloaded => buf.put_u8(E_OVERLOADED),
        RemoteError::NoReply => buf.put_u8(E_NO_REPLY),
        RemoteError::Decode(reason) => {
            buf.put_u8(E_DECODE);
            buf.put_slice(reason.as_bytes());
        }
        RemoteError::Encode(reason) => {
            buf.put_u8(E_ENCODE);
            buf.put_slice(reason.as_bytes());
        }
        RemoteError::TooLarge => buf.put_u8(E_TOO_LARGE),
    }
}

fn get_error(mut bytes: Bytes) -> Option<RemoteError> {
    Some(match get_u8(&mut bytes)? {
        E_UNKNOWN_MESSAGE => RemoteError::UnknownMessage,
        E_NO_SUCH_ACTOR => RemoteError::NoSuchActor,
        E_NOT_ACCEPTED => RemoteError::NotAccepted,
        E_CLOSED => RemoteError::Closed,
        E_OVERLOADED => RemoteError::Overloaded,
        E_NO_REPLY => RemoteError::NoReply,
        E_DECODE => RemoteError::Decode(String::from_utf8_lossy(&bytes).into_owned()),
        E_ENCODE => RemoteError::Encode(String::from_utf8_lossy(&bytes).into_owned()),
        E_TOO_LARGE => RemoteError::TooLarge,
        _ => return None,
    })
}

fn get_u8(bytes: &mut Bytes) -> Option<u8> {
    bytes.has_remaining().then(|| bytes.get_u8())
}

fn get_u16(bytes: &mut Bytes) -> Option<u16> {
    (bytes.remaining() >= 2).then(|| bytes.get_u16())
}

fn get_u64(bytes: &mut Bytes) -> Option<u64> {
    (bytes.remaining() >= 8).then(|| bytes.get_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trips(frame: Frame) {
        assert_eq!(Frame::decode(frame.encode()), Some(frame));
    }

    #[test]
    fn frames_round_trip() {
        let (target, msg) = (Pid::new("counter"), Id::from_u128(0xabcdef));
        round_trips(Frame::Cast {
            target: target.clone(),
            msg,
            payload: Bytes::from_static(b"payload"),
        });
        round_trips(Frame::Call {
            call_id: 42,
            target,
            msg,
            payload: Bytes::new(),
        });
        round_trips(Frame::Reply {
            call_id: 42,
            result: Ok(Bytes::from_static(b"answer")),
        });
    }

    #[test]
    fn every_error_round_trips() {
        for error in [
            RemoteError::UnknownMessage,
            RemoteError::NoSuchActor,
            RemoteError::NotAccepted,
            RemoteError::Closed,
            RemoteError::Overloaded,
            RemoteError::NoReply,
            RemoteError::Decode("bad".into()),
            RemoteError::Encode("worse".into()),
            RemoteError::TooLarge,
        ] {
            round_trips(Frame::Reply {
                call_id: 1,
                result: Err(error),
            });
        }
    }

    #[test]
    fn garbage_is_not_a_frame() {
        assert_eq!(Frame::decode(Bytes::new()), None);
        assert_eq!(Frame::decode(Bytes::from_static(&[0xff])), None);
        // Cut short at every point.
        let frame = Frame::Call {
            call_id: 7,
            target: Pid::new("counter"),
            msg: Id::from_u128(1),
            payload: Bytes::new(),
        }
        .encode();
        for len in 0..frame.len() {
            assert_eq!(Frame::decode(frame.slice(..len)), None, "cut at {len}");
        }
    }
}
