# Changelog

## Unreleased

This is a **breaking** change for anyone constructing `ChildConfig` with a
struct literal, and for anyone relying on a supervisor exiting successfully
when it gives up. `zestors` is not published for this change: pin the commit
by its git SHA.

### Supervision

- **Restart backoff.** A child's restarts are now delayed, growing
  exponentially from 100 ms, doubling per consecutive restart and capped at
  30 seconds. The delay is configured per child with the new
  `RestartBackoff` type (`ChildConfig::backoff`, `ChildSpec::with_backoff`);
  `RestartBackoff::none()` restores immediate restarts. A child that
  initializes successfully again resets its backoff.
- **Start-up errors are logged.** A child that exits before finishing
  initialization is now logged at `warn` with its name and the error it
  returned (previously only "Supervisee exited before finishing
  initialization" was logged, without the child's own error).
- **Exhausted restart budgets escalate.** When a supervisor (or a child)
  exhausts its `RestartIntensity`, the supervisor stops its children and exits
  with an error naming the child, instead of exiting successfully. A `Node`
  turns this into `NodeError::SupervisorExited` (exit code 1), and a parent
  supervisor sees an ordinary child failure and can restart or escalate in
  turn.
- **Fixed: rest-for-one could retry forever.** The rest-for-one cascade-retry
  path only checked the per-child restart budget, not the supervisor-wide one.
  With the default per-child intensity (`None`), a child that crashed and then
  failed every restart was restarted in an unbounded loop. It now counts
  against the supervisor's budget and gives up like every other strategy.

The `ChildConfig` struct gained the `backoff` field; code constructing it with
a struct literal must add `backoff` or use `..Default::default()`.
