use futures::{FutureExt, Stream};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub struct FutureIdMap<F> {
    next_id: usize,
    futures: Vec<(u64, F)>,
}

impl<F> FutureIdMap<F> {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            futures: Vec::new(),
        }
    }
}

impl<F> Unpin for FutureIdMap<F> {}

impl<F: Future + Unpin> Stream for FutureIdMap<F> {
    type Item = (u64, F::Output);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = self
            .futures
            .iter_mut()
            .enumerate()
            .find_map(|(index, future)| {
                if let Poll::Ready(output) = future.1.poll_unpin(cx) {
                    Some((index, output))
                } else {
                    None
                }
            });

        match result {
            Some((index, output)) => {
                let (fut_id, _future) = self.futures.swap_remove(index);
                Poll::Ready(Some((fut_id, output)))
            }
            None => {
                if self.futures.len() > 0 {
                    Poll::Pending
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}
