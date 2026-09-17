//! The message/response types used to interact with a running
//! [`Supervisor`]: fetching its children
//! ([`GetChildren`]), checking its health ([`GetHealth`]), and registering
//! or deregistering a child at runtime ([`RegisterChild`]/
//! [`DeregisterChild`]).

use crate::_prelude::*;
use smol_str::{SmolStr, format_smolstr};
use zestors_codegen::Message;
use zestors_runtime::errors::DuplicatePidError;

/// Requests the [`ChildDescription`] of every direct child of a supervisor.
#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "Vec<ChildDescription>")]
pub struct GetChildren;

/// Requests a supervisor's current [`Health`].
#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = Health)]
pub struct GetHealth;

/// Registers a new child under a running supervisor. Fails if the spec's
/// [`Pid`] is already registered.
#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "Result<(), DuplicatePidError>")]
pub struct RegisterChild(pub ChildSpec);

/// Removes a child from a running supervisor (stopping it if it's alive),
/// returning its [`ChildDescription`] if it was present.
#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "Option<ChildDescription>")]
pub struct DeregisterChild(pub Pid);

/// A point-in-time health report, as returned by [`GetHealth`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Health {
    /// The overall status.
    pub status: HealthStatus,
    /// An optional `{:?}`-formatted snapshot of whatever reported this
    /// health, attached via [`Health::with_debug_repr`].
    pub debug_repr: Option<SmolStr>,
    /// An optional human-readable summary.
    pub message: Option<SmolStr>,
    /// Finer-grained details backing the overall status.
    pub details: Vec<HealthDetail>,
}

impl Health {
    /// Creates a [`Health`] with `status` and nothing else set.
    pub fn new(status: HealthStatus) -> Self {
        Self {
            status,
            debug_repr: None,
            message: None,
            details: Vec::new(),
        }
    }

    /// Shorthand for [`Health::new`]`(`[`HealthStatus::Healthy`]`)`.
    pub fn healthy() -> Self {
        Self::new(HealthStatus::Healthy)
    }

    /// Shorthand for [`Health::new`]`(`[`HealthStatus::Degraded`]`)`.
    pub fn degraded() -> Self {
        Self::new(HealthStatus::Degraded)
    }

    /// Shorthand for [`Health::new`]`(`[`HealthStatus::Unhealthy`]`)`.
    pub fn unhealthy() -> Self {
        Self::new(HealthStatus::Unhealthy)
    }

    /// Sets [`Health::debug_repr`] to `debug_repr`, `{:?}`-formatted.
    pub fn add_debug_repr(&mut self, debug_repr: impl Debug) {
        self.debug_repr = Some(format_smolstr!("{:?}", debug_repr));
    }

    /// Builder-style version of [`Health::add_debug_repr`].
    pub fn with_debug_repr(mut self, debug_repr: impl Debug) -> Self {
        self.add_debug_repr(debug_repr);
        self
    }

    /// Sets [`Health::message`], returning the previous value if any.
    pub fn add_message(&mut self, message: impl Into<SmolStr>) -> Option<SmolStr> {
        let old = self.message.take();
        self.message = Some(message.into());
        old
    }

    /// Builder-style version of [`Health::add_message`].
    pub fn with_message(mut self, message: impl Into<SmolStr>) -> Self {
        self.add_message(message);
        self
    }

    /// Appends `detail` to [`Health::details`].
    pub fn add_detail(&mut self, detail: HealthDetail) {
        self.details.push(detail);
    }

    /// Builder-style version of [`Health::add_detail`].
    pub fn with_detail(mut self, detail: HealthDetail) -> Self {
        self.add_detail(detail);
        self
    }

    /// Appends `details` to [`Health::details`].
    pub fn add_details(&mut self, details: impl IntoIterator<Item = HealthDetail>) {
        self.details.extend(details);
    }

    /// Builder-style version of [`Health::add_details`].
    pub fn with_details(mut self, details: impl IntoIterator<Item = HealthDetail>) -> Self {
        self.add_details(details);
        self
    }
}

impl Display for Health {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.status)?;

        if let Some(message) = &self.message {
            write!(f, ": {}", message)?;
        }

        Ok(())
    }
}

impl From<HealthStatus> for Health {
    fn from(status: HealthStatus) -> Self {
        Health::new(status)
    }
}

/// The overall status a [`Health`] report describes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    /// Fully functional.
    Healthy,
    /// Functional, but with a known issue worth surfacing.
    Degraded,
    /// Not functioning correctly.
    Unhealthy,
}

impl Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl HealthStatus {
    /// Returns `true` if this is [`HealthStatus::Healthy`].
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    /// Returns `true` if this is [`HealthStatus::Degraded`].
    pub fn is_degraded(&self) -> bool {
        matches!(self, HealthStatus::Degraded)
    }

    /// Returns `true` if this is [`HealthStatus::Unhealthy`].
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy)
    }

    /// A lowercase, human-readable name for this status.
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
        }
    }

    /// Wraps this status in a [`Health`] with nothing else set.
    pub fn into_health(self) -> Health {
        Health::new(self)
    }
}

/// A single fact backing a [`Health`] report's overall status, e.g. "database:
/// connection pool exhausted".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthDetail {
    /// What this detail is about, e.g. a subsystem name.
    pub name: SmolStr,
    /// A human-readable description.
    pub message: SmolStr,
    /// When this detail became true, if known.
    pub since: Option<jiff::Zoned>,
}

impl HealthDetail {
    /// Creates a detail with `name` and `message`, and no [`HealthDetail::since`].
    pub fn new(name: impl Into<SmolStr>, message: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
            since: None,
        }
    }

    /// Sets [`HealthDetail::since`].
    pub fn with_since(mut self, since: jiff::Zoned) -> Self {
        self.since = Some(since);
        self
    }
}

impl Display for HealthDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)?;
        if let Some(since) = &self.since {
            write!(f, " (since {})", since)?;
        }
        Ok(())
    }
}
