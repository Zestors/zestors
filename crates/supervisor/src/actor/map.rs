use super::*;
use std::{pin::Pin, task::ready};
use streamunordered::{StreamUnordered, StreamYield};
use zestors_runtime::errors::DuplicateNameError;
use zestors_supervision::ChildDescription;

#[derive(Debug, Default)]
pub(super) struct SuperviseeMap {
    mapping: IndexMap<Name, SuperviseeMapEntry>,
    supervisees: Pin<Box<StreamUnordered<Supervisee>>>,
}

#[derive(Debug)]
struct SuperviseeMapEntry {
    token: Option<usize>,
}

impl SuperviseeMapEntry {
    fn new(token: usize) -> Self {
        Self { token: Some(token) }
    }
}

impl SuperviseeMap {
    pub(super) fn new(specs: impl IntoIterator<Item = ChildSpec>) -> Self {
        let mut this = Self::default();

        for spec in specs {
            let supervisee = Supervisee::new(spec);
            let name = supervisee.name().clone();

            let token = this.supervisees.insert(supervisee);
            this.mapping.insert(name, SuperviseeMapEntry::new(token));
        }

        this
    }

    pub(super) fn start_all(&mut self) -> Result<(), (Name, SuperviseeIsShuttingDown)> {
        for (name, entry) in self.mapping.iter() {
            if let Some(token) = entry.token {
                let supervisee = self.supervisees.get_mut(token).expect("Should exist");
                if let Err(e) = supervisee.start() {
                    return Err((name.clone(), e));
                }
            }
        }

        Ok(())
    }

    /// Stops every supervisee, returning the ones that are still alive
    /// afterward (and thus still owe an exit event to wait for).
    #[must_use]
    pub(super) fn stop_all(&mut self) -> Vec<Name> {
        self.mapping
            .iter()
            .filter_map(|(name, entry)| {
                let supervisee = self
                    .supervisees
                    .get_mut(entry.token?)
                    .expect("Should exist");
                supervisee.stop().is_shutting_down().then(|| name.clone())
            })
            .collect()
    }

    fn supervisees(&self) -> impl Iterator<Item = &Supervisee> {
        self.mapping.values().filter_map(|entry| {
            entry
                .token
                .map(|token| self.supervisees.get(token).unwrap())
        })
    }

    pub(super) fn addresses(&self) -> impl Iterator<Item = &Address> {
        self.supervisees().map(|s| s.address())
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &Name> {
        self.addresses().map(|a| a.name())
    }

    pub(super) fn child_descriptions(&self) -> Vec<ChildDescription> {
        self.supervisees()
            .map(|supervisee| supervisee.get_description())
            .collect()
    }

    pub(super) fn get_mut<'a>(&'a mut self, name: &Name) -> Option<&'a mut Supervisee> {
        self.mapping.get(name).and_then(|entry| {
            entry
                .token
                .map(|token| self.supervisees.get_mut(token).unwrap())
        })
    }

    pub(super) fn remove(&mut self, name: &Name) -> Option<Supervisee> {
        match self.mapping.get_mut(name) {
            Some(entry) => {
                if let Some(token) = entry.token.take() {
                    let removed = self.supervisees.as_mut().take(token);
                    Some(removed.expect("Should be there"))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub(super) fn add(&mut self, supervisee: Supervisee) -> Result<(), DuplicateNameError> {
        if self
            .mapping
            .get(supervisee.name())
            .is_some_and(|entry| entry.token.is_some())
        {
            return Err(DuplicateNameError {
                name: supervisee.name().clone(),
            });
        }

        let name = supervisee.name().clone();
        let token = self.supervisees.as_mut().insert(supervisee);
        self.mapping.insert(name, SuperviseeMapEntry::new(token));

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
