use super::*;
use crate::SuperviseeNext;
use std::{pin::Pin, task::ready};
use streamunordered::{StreamUnordered, StreamYield};
use zestors_runtime::errors::DuplicatePidError;

#[derive(Debug, Default)]
pub(super) struct SuperviseeMap {
    mapping: IndexMap<Pid, Option<usize>>,
    supervisees: Pin<Box<StreamUnordered<Supervisee>>>,
}

impl SuperviseeMap {
    pub(super) fn new(specs: impl IntoIterator<Item = ChildSpec>) -> Self {
        let mut this = Self::default();

        for spec in specs {
            let supervisee = Supervisee::new(spec);
            let pid = supervisee.pid().clone();

            let token = this.supervisees.insert(supervisee);
            this.mapping.insert(pid, Some(token));
        }

        this
    }

    pub(super) fn start_all(&mut self) -> Result<(), (Pid, SuperviseeIsShuttingDown)> {
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

    /// Stops every supervisee, returning the ones that are still alive
    /// afterward (and thus still owe an exit event to wait for).
    #[must_use]
    pub(super) fn stop_all(&mut self) -> Vec<Pid> {
        self.mapping
            .iter()
            .filter_map(|(pid, token)| {
                let supervisee = self.supervisees.get_mut((*token)?).expect("Should exist");
                supervisee.stop().is_shutting_down().then(|| pid.clone())
            })
            .collect()
    }

    fn supervisees(&self) -> impl Iterator<Item = &Supervisee> {
        self.mapping
            .values()
            .filter_map(|token| token.map(|token| self.supervisees.get(token).unwrap()))
    }

    pub(super) fn addresses(&self) -> impl Iterator<Item = &Address> {
        self.supervisees().map(|s| s.address())
    }

    pub(super) fn pids(&self) -> impl Iterator<Item = &Pid> {
        self.addresses().map(|a| a.pid())
    }

    pub(super) fn child_descriptions(&self) -> Vec<ChildDescription> {
        self.supervisees()
            .map(|supervisee| supervisee.get_description())
            .collect()
    }

    pub(super) fn get_mut<'a>(&'a mut self, pid: &Pid) -> Option<&'a mut Supervisee> {
        self.mapping
            .get(pid)
            .and_then(|token| token.map(|token| self.supervisees.get_mut(token).unwrap()))
    }

    pub(super) fn remove(&mut self, pid: &Pid) -> Option<Supervisee> {
        match self.mapping.get_mut(pid) {
            Some(token) => {
                if let Some(token) = token.take() {
                    let removed = self.supervisees.as_mut().take(token);
                    Some(removed.expect("Should be there"))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub(super) fn add(&mut self, supervisee: Supervisee) -> Result<(), DuplicatePidError> {
        if self
            .mapping
            .get(supervisee.pid())
            .is_some_and(|token| token.is_some())
        {
            return Err(DuplicatePidError {
                pid: supervisee.pid().clone(),
            });
        }

        let pid = supervisee.pid().clone();
        let token = self.supervisees.as_mut().insert(supervisee);
        self.mapping.insert(pid, Some(token));

        Ok(())
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
