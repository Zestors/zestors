use crate::{registry::Registry, *};
use bs58::Alphabet;
use smol_str::SmolStr;
use std::{borrow::Cow, fmt::Display, sync::Arc};
use type_sets::{AsTypeSet, Members};

/// A process identifier: a cheaply-cloneable, human-readable string that
/// uniquely names an actor in the local [`Registry`].
///
/// A `Pid` is stable across restarts: creating a new [`StrongAddress`] with a
/// given `Pid` (see [`StrongAddress::create`]) reuses the same registry entry,
/// which is what allows an actor to be restarted on the same [`Channel`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pid(SmolStr);

impl Pid {
    /// Creates a `Pid` from any type that converts into one (a `String`,
    /// `&'static str`, etc.).
    pub fn new<T: Into<Self>>(s: T) -> Self {
        s.into()
    }

    /// Creates a `Pid` from a `&'static str` without allocating.
    pub fn new_static(s: &'static str) -> Self {
        Pid(SmolStr::new_static(s))
    }

    /// Generates a new `Pid` from random bytes, base58-encoded.
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

    /// Looks up the untyped [`Address`] registered for this `Pid`, or `None`
    /// if no actor with this `Pid` is currently registered.
    pub fn address(&self) -> Option<Address> {
        Registry::local().get(&self)
    }

    /// Looks up the [`Address`] registered for this `Pid`, downcast to the
    /// given [`Interface`]. See [`Registry::get_typed`].
    pub fn typed_address<I: Interface>(&self) -> Result<Address<I>, TypedRegistryError> {
        Registry::local().get_typed::<I>(self)
    }

    /// Looks up the [`Address`] registered for this `Pid`, downcast to the
    /// given dynamic message set. See [`Registry::get_dyn`].
    pub fn dyn_address<S>(&self) -> Result<Address<Dyn<S>>, TypedRegistryError>
    where
        S: AsTypeSet + 'static + Members,
    {
        Registry::local().get_dyn::<S>(self)
    }

    /// Returns the [`Pid`] of the actor currently running on this task, or
    /// `None` if not called from within an actor's task.
    pub fn current() -> Option<Self> {
        crate::current_pid()
    }

    /// Returns the [`Pid`] of the actor that spawned the actor currently
    /// running on this task, or `None` if not called from within an actor's
    /// task, or if that actor has no parent.
    pub fn parent() -> Option<Self> {
        crate::parent_pid()
    }
}

impl Default for Pid {
    fn default() -> Self {
        Pid::rand()
    }
}

impl Display for Pid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&'static str> for Pid {
    #[inline]
    fn from(s: &'static str) -> Self {
        Pid(SmolStr::new_static(s))
    }
}

impl From<&mut str> for Pid {
    #[inline]
    fn from(s: &mut str) -> Self {
        Pid(SmolStr::from(s))
    }
}

impl From<&String> for Pid {
    #[inline]
    fn from(s: &String) -> Self {
        Pid(SmolStr::from(s))
    }
}

impl From<String> for Pid {
    #[inline(always)]
    fn from(text: String) -> Self {
        Pid(SmolStr::from(text))
    }
}

impl From<Box<str>> for Pid {
    #[inline]
    fn from(s: Box<str>) -> Pid {
        Pid(SmolStr::from(s))
    }
}

impl From<Arc<str>> for Pid {
    #[inline]
    fn from(s: Arc<str>) -> Pid {
        Pid(SmolStr::from(s))
    }
}

impl<'a> From<Cow<'a, str>> for Pid {
    #[inline]
    fn from(s: Cow<'a, str>) -> Pid {
        Pid(SmolStr::from(s))
    }
}

impl From<Pid> for String {
    #[inline]
    fn from(pid: Pid) -> String {
        pid.0.into()
    }
}

impl From<&Pid> for String {
    #[inline]
    fn from(pid: &Pid) -> String {
        pid.0.to_string()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_pid() {
        for i in 0..100 {
            let pid = Pid::rand();
            println!("pid {}: {}", i, pid);
        }

        let pid1 = Pid::rand();
        let pid2 = Pid::rand();
        assert_ne!(pid1, pid2);
    }
}
