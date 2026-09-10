use super::*;
use crate::SuperviseeNext;
use std::{pin::Pin, task::ready};
use streamunordered::{StreamUnordered, StreamYield};

#[derive(Debug)]
pub(super) struct SuperviseeMap {
    mapping: IndexMap<Pid, Option<usize>>,
    supervisees: Pin<Box<StreamUnordered<Supervisee>>>,
}

impl SuperviseeMap {
    pub fn start_all(&mut self) -> Result<(), (Pid, SuperviseeStartError)> {
        for (pid, token) in self.mapping.iter() {
            if let Some(token) = token {
                let supervisee = self.supervisees.get_mut(*token).expect("Should exist");
                if let Err(e) = supervisee.start() {
                    return Err((pid.clone(), e));
                }
            }
        }

        Ok(())
    }

    pub fn stop_all(&mut self) {
        for (_pid, token) in self.mapping.iter() {
            if let Some(token) = token {
                let supervisee = self.supervisees.get_mut(*token).expect("Should exist");
                supervisee.stop();
            }
        }
    }

    fn supervisees(&self) -> impl Iterator<Item = &Supervisee> {
        self.mapping
            .values()
            .filter_map(|token| token.map(|token| self.supervisees.get(token).unwrap()))
    }

    pub fn addresses(&self) -> impl Iterator<Item = &Address> {
        self.supervisees().map(|s| s.address())
    }

    pub fn child_descriptions(&self) -> Vec<ChildDescription> {
        self.supervisees()
            .map(|supervisee| supervisee.get_description())
            .collect()
    }

    pub fn get(&self, pid: &Pid) -> Option<&Supervisee> {
        self.mapping
            .get(pid)
            .and_then(|token| token.map(|token| self.supervisees.get(token).unwrap()))
    }

    pub fn remove(&mut self, pid: &Pid) -> bool {
        match self.mapping.get_mut(pid) {
            Some(token) => {
                if let Some(token) = token.take() {
                    let was_removed = self.supervisees.as_mut().remove(token);
                    assert!(was_removed);
                }

                true
            }
            None => false,
        }
    }
}

impl Stream for SuperviseeMap {
    type Item = SuperviseeNext;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.supervisees.poll_next_unpin(cx)) {
            Some((yielded, _token)) => match yielded {
                StreamYield::Item(item) => Poll::Ready(Some(item)),
                StreamYield::Finished(_) => {
                    unreachable!("Supervisee's `next` never returns None")
                }
            },
            None => Poll::Ready(None),
        }
    }
}
