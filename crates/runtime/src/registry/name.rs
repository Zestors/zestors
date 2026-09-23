use crate::{registry::Registry, *};
use bs58::Alphabet;
use smol_str::SmolStr;
use std::{borrow::Cow, fmt::Display, sync::Arc};

/// The name of an actor: a cheaply-cloneable, human-readable string that
/// uniquely names an actor in the local [`Registry`].
///
/// A `Name` is stable across restarts: a supervisor restarts an actor by
/// spawning a new task on the same [`StrongAddress`] (see
/// [`StrongAddress::spawn`]), which keeps its name and registry entry. Creating
/// a second channel under a name that is in use fails.
///
/// A `Name` is unique within one process. To name an actor on another node of
/// a cluster, `zestors-distr` pairs it with the node's name in a
/// `GlobalName`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Name(SmolStr);

impl Name {
    /// Creates a `Name` from any type that converts into one (a `String`,
    /// `&'static str`, etc.).
    pub fn new<T: Into<Self>>(s: T) -> Self {
        s.into()
    }

    /// Creates a `Name` from a `&'static str` without allocating.
    pub fn new_static(s: &'static str) -> Self {
        Name(SmolStr::new_static(s))
    }

    /// Generates a new `Name` from random bytes, base58-encoded.
    pub fn rand() -> Self {
        let rand: [u8; 11] = rand::random();

        let mut val = String::with_capacity(16);
        bs58::encode(rand)
            .with_alphabet(&Alphabet::BITCOIN)
            .onto(&mut val)
            .expect("Capacity is sufficient");
        val.truncate(14);

        Self::new(val)
    }

    /// Looks up the untyped [`Address`] registered for this `Name`, or `None`
    /// if no actor with this `Name` is currently registered.
    pub fn address(&self) -> Option<Address> {
        Registry::local().get(&self)
    }

    // /// Returns the [`Name`] of the actor currently running on this task, or
    // /// `None` if not called from within an actor's task.
    // pub fn current() -> Option<Self> {
    //     crate::current_name()
    // }

    // /// Returns the [`Name`] of the actor that spawned the actor currently
    // /// running on this task, or `None` if not called from within an actor's
    // /// task, or if that actor has no parent.
    // pub fn parent() -> Option<Self> {
    //     crate::parent_name()
    // }
}

impl Default for Name {
    fn default() -> Self {
        Name::rand()
    }
}

impl Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&'static str> for Name {
    #[inline]
    fn from(s: &'static str) -> Self {
        Name(SmolStr::new_static(s))
    }
}

impl From<&mut str> for Name {
    #[inline]
    fn from(s: &mut str) -> Self {
        Name(SmolStr::from(s))
    }
}

impl From<&String> for Name {
    #[inline]
    fn from(s: &String) -> Self {
        Name(SmolStr::from(s))
    }
}

impl From<String> for Name {
    #[inline(always)]
    fn from(text: String) -> Self {
        Name(SmolStr::from(text))
    }
}

impl From<Box<str>> for Name {
    #[inline]
    fn from(s: Box<str>) -> Name {
        Name(SmolStr::from(s))
    }
}

impl From<Arc<str>> for Name {
    #[inline]
    fn from(s: Arc<str>) -> Name {
        Name(SmolStr::from(s))
    }
}

impl<'a> From<Cow<'a, str>> for Name {
    #[inline]
    fn from(s: Cow<'a, str>) -> Name {
        Name(SmolStr::from(s))
    }
}

impl From<Name> for String {
    #[inline]
    fn from(name: Name) -> String {
        name.0.into()
    }
}

impl From<&Name> for String {
    #[inline]
    fn from(name: &Name) -> String {
        name.0.to_string()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_name() {
        for i in 0..100 {
            let name = Name::rand();
            println!("name {}: {}", i, name);
        }

        let name1 = Name::rand();
        let name2 = Name::rand();
        assert_ne!(name1, name2);
    }
}
