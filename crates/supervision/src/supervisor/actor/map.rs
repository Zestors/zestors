use super::*;
use crate::SuperviseeNext;
use std::{ops::Not, pin::Pin, task::ready};
use streamunordered::{StreamUnordered, StreamYield};
use zestors_runtime::errors::DuplicatePidError;

#[derive(Debug, Default)]
pub(super) struct SuperviseeMap {
    mapping: IndexMap<Pid, Option<usize>>,
    supervisees: Pin<Box<StreamUnordered<Supervisee>>>,
}

impl SuperviseeMap {
    pub fn new(specs: impl IntoIterator<Item = ChildSpec>) -> Self {
        let mut this = Self::default();

        for spec in specs {
            let supervisee = Supervisee::new(spec);
            let pid = supervisee.pid().clone();

            let token = this.supervisees.insert(supervisee);
            this.mapping.insert(pid, Some(token));
        }

        this
    }

    pub fn first_mut(&mut self) -> Option<&mut Supervisee> {
        let token = self.mapping.values().find_map(|token| *token);
        token.map(|token| self.supervisees.get_mut(token).unwrap())
    }

    pub fn next_mut<'a>(&'a mut self, pid: &Pid) -> Option<&'a mut Supervisee> {
        let idx = self.mapping.get_index_of(pid)?;
        let (_pid, token) = self.mapping.get_index_mut(idx + 1)?;
        let token = (*token)?;
        self.supervisees.get_mut(token)
    }

    pub fn prev_mut<'a>(&'a mut self, pid: &Pid) -> Option<&'a mut Supervisee> {
        let idx = self.mapping.get_index_of(pid)?;
        if idx == 0 {
            return None;
        }
        let (_pid, token) = self.mapping.get_index_mut(idx - 1)?;
        let token = (*token)?;
        self.supervisees.get_mut(token)
    }

    pub fn start_all(&mut self) -> Result<(), (Pid, SuperviseeIsShuttingDown)> {
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

    fn get_token(&self, pid: &Pid) -> Option<usize> {
        self.mapping.get(pid).cloned().flatten()
    }

    #[must_use]
    pub fn restart_all(&mut self) -> Vec<Pid> {
        self.mapping
            .iter()
            .filter_map(|(pid, token)| token.map(|token| (pid, token)))
            .filter_map(|(pid, token)| {
                let supervisee = self.supervisees.get_mut(token).unwrap();

                supervisee.start().ok().and_then(|_| {
                    matches!(
                        supervisee.status(),
                        SuperviseeStatus::Initializing | SuperviseeStatus::Starting
                    )
                    .then_some(pid.clone())
                })
            })
            .collect()
    }

    #[must_use]
    pub fn stop_pids(
        &mut self,
        pids: impl IntoIterator<Item = Pid>,
    ) -> Result<Vec<Pid>, RestartLimitReached> {
        pids.into_iter()
            .filter_map(|pid| {
                let Some(token) = self.get_token(&pid) else {
                    return None;
                };

                let supervisee = self.supervisees.get_mut(token).expect("Should exist");

                if !supervisee.acquire_restart_permit() {
                    return Some(Err(RestartLimitReached { pid }));
                }

                supervisee.stop();

                matches!(supervisee.status(), SuperviseeStatus::Dead)
                    .not()
                    .then_some(Ok(pid))
            })
            .collect()
    }

    /// Stops every supervisee, returning the ones that are still alive
    /// afterward (and thus still owe an exit event to wait for). A
    /// supervisee stopped while `Starting` is cancelled synchronously and
    /// lands directly on `Dead` with no further event, so its aliveness has
    /// to be checked *after* calling `stop`, not inferred from `stop`'s own
    /// return value.
    #[must_use]
    pub fn stop_all(&mut self) -> Vec<Pid> {
        self.mapping
            .iter()
            .filter_map(|(pid, token)| {
                let supervisee = self.supervisees.get_mut((*token)?).expect("Should exist");
                supervisee.stop();
                (supervisee.status() != SuperviseeStatus::Dead).then(|| pid.clone())
            })
            .collect()
    }

    fn supervisees(&self) -> impl Iterator<Item = &Supervisee> {
        self.mapping
            .values()
            .filter_map(|token| token.map(|token| self.supervisees.get(token).unwrap()))
    }

    pub fn addresses(&self) -> impl Iterator<Item = &Address> {
        self.supervisees().map(|s| s.address())
    }

    pub fn pids(&self) -> impl Iterator<Item = &Pid> {
        self.addresses().map(|a| a.pid())
    }

    pub fn child_descriptions(&self) -> Vec<ChildDescription> {
        self.supervisees()
            .map(|supervisee| supervisee.get_description())
            .collect()
    }

    pub fn get<'a>(&'a self, pid: &Pid) -> Option<&'a Supervisee> {
        self.mapping
            .get(pid)
            .and_then(|token| token.map(|token| self.supervisees.get(token).unwrap()))
    }

    pub fn get_mut<'a>(&'a mut self, pid: &Pid) -> Option<&'a mut Supervisee> {
        self.mapping
            .get(pid)
            .and_then(|token| token.map(|token| self.supervisees.get_mut(token).unwrap()))
    }

    pub fn remove(&mut self, pid: &Pid) -> Option<Supervisee> {
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

    pub fn add(&mut self, supervisee: Supervisee) -> Result<(), DuplicatePidError> {
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

#[derive(Debug, thiserror::Error)]
#[error("Pid {pid} reached the restart-limit")]
pub struct RestartLimitReached {
    pid: Pid,
}
