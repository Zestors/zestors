use futures::{Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct StreamMap<F> {
    next_id: usize,
    streams: Vec<(u64, F)>,
}

impl<F> StreamMap<F> {}

impl<F> Unpin for StreamMap<F> {}

impl<F: Stream + Unpin> Stream for StreamMap<F> {
    type Item = (u64, F::Item);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let result = self
                .streams
                .iter_mut()
                .enumerate()
                .find_map(|(index, stream)| {
                    if let Poll::Ready(item) = stream.1.poll_next_unpin(cx) {
                        Some((index, stream.0, item))
                    } else {
                        None
                    }
                });

            match result {
                Some((index, stream_id, item)) => match item {
                    Some(item) => {
                        break Poll::Ready(Some((stream_id, item)));
                    }
                    None => {
                        let _ = self.streams.swap_remove(index);
                    }
                },
                None => {
                    if self.streams.len() > 0 {
                        break Poll::Pending;
                    } else {
                        break Poll::Ready(None);
                    }
                }
            }
        }
    }
}
