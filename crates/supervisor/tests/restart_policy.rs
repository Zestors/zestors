//! Tests for the supervisor's restart policy: how it logs a child that fails
//! before initialization, backs off between restarts, resets that backoff once
//! a child has proven itself by staying up, and gives up once its restart
//! budget is exhausted.

use rootcause::Report;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tracing::Level;
use zestors_actor::{Actor, RestartMode, fn_blueprint};
use zestors_runtime::{Child, Dyn, Inbox, prelude::*};
use zestors_supervision::{
    BlueprintSupervisionExt as _, ChildConfig, ChildSpec, RestartBackoff, RestartIntensity,
};
use zestors_supervisor::SupervisorBlueprint;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for the paused-time tests: virtual, so it only bounds restart loops
/// instead of how long the test really runs.
const PAUSED_TIMEOUT: Duration = Duration::from_secs(120);

/// A child that always fails before it can finish initializing.
struct AlwaysFails {
    spawns: Arc<AtomicUsize>,
    #[allow(dead_code)]
    spawn_times: Option<Arc<Mutex<Vec<Instant>>>>,
}

impl Actor for AlwaysFails {
    type Interface = ();
    type Exit = ();

    async fn run(self, _inbox: Inbox<()>) -> Result<(), Report> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        if let Some(times) = &self.spawn_times {
            times.lock().unwrap().push(Instant::now());
        }
        Err(rootcause::report!("bind failed: address already in use"))
    }
}

fn always_fails(
    name: &str,
    cfg: ChildConfig,
    spawn_times: Option<Arc<Mutex<Vec<Instant>>>>,
) -> (ChildSpec, Arc<AtomicUsize>) {
    let name: Name = name.to_string().into();
    let spawns = Arc::new(AtomicUsize::new(0));
    let for_actor = spawns.clone();

    let (spec, _) = fn_blueprint(move || AlwaysFails {
        spawns: for_actor.clone(),
        spawn_times: spawn_times.clone(),
    })
    .name(name)
    .expect("name is unique")
    .with_cfg(cfg)
    .split();

    (spec, spawns)
}

fn cfg(intensity: Option<RestartIntensity>, backoff: RestartBackoff) -> ChildConfig {
    ChildConfig {
        restart_mode: RestartMode::Always,
        intensity,
        backoff,
        abort_timeout: Duration::from_secs(1),
        init_timeout: Duration::from_secs(1),
        start_timeout: Duration::from_secs(1),
    }
}

async fn spawn_supervisor(blueprint: SupervisorBlueprint) -> Child<(), Dyn> {
    let (spec, _) = blueprint.rand_name().split();
    spec.start().await.expect("supervisor should start")
}

/// Captures everything the `tracing` subscriber is given, so a test can assert
/// on the supervisor's log output.
#[derive(Clone, Default)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl CapturedLog {
    fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl std::io::Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLog {
    type Writer = CapturedLog;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn startup_failure_is_logged_with_child_name_and_error() {
    let log = CapturedLog::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::WARN)
        .with_ansi(false)
        .without_time()
        .with_writer(log.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let (spec, _spawns) = always_fails(
        "bind-fail",
        cfg(
            Some(RestartIntensity::restarts(1).within(Duration::from_secs(30))),
            RestartBackoff::none(),
        ),
        None,
    );
    let mut supervisor = spawn_supervisor(SupervisorBlueprint::one_for_one().child(spec)).await;

    let result = tokio::time::timeout(TIMEOUT, &mut supervisor).await;
    assert!(result.expect("supervisor should give up promptly").is_err());

    let log = log.contents();
    assert!(
        log.contains("bind-fail"),
        "the log should name the child that failed to initialize:\n{log}"
    );
    assert!(
        log.contains("bind failed: address already in use"),
        "the log should contain the child's own start-up error:\n{log}"
    );
}

#[tokio::test]
async fn restart_backoff_grows_exponentially_and_is_capped() {
    let spawn_times = Arc::new(Mutex::new(Vec::new()));
    let (spec, spawns) = always_fails(
        "backing-off",
        cfg(
            None,
            RestartBackoff::new(Duration::from_millis(40), Duration::from_millis(160)),
        ),
        Some(spawn_times.clone()),
    );

    // The initial start plus four restarts: delays 40, 80, 160, 160 ms.
    let mut supervisor = spawn_supervisor(
        SupervisorBlueprint::one_for_one()
            .intensity(RestartIntensity::new(4, Duration::from_secs(60)))
            .child(spec),
    )
    .await;

    let result = tokio::time::timeout(TIMEOUT, &mut supervisor).await;
    assert!(
        result.expect("supervisor should give up promptly").is_err(),
        "the supervisor should propagate its exhausted restart budget"
    );
    assert_eq!(spawns.load(Ordering::SeqCst), 5);

    let times = spawn_times.lock().unwrap().clone();
    let gaps: Vec<Duration> = times.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(gaps.len(), 4, "expected four restarts: {times:?}");

    let expected = [
        Duration::from_millis(40),
        Duration::from_millis(80),
        Duration::from_millis(160),
        Duration::from_millis(160),
    ];
    for (i, (gap, expected)) in gaps.iter().zip(expected).enumerate() {
        assert!(
            *gap >= expected.mul_f32(0.8),
            "restart {i} waited {gap:?}, expected at least about {expected:?}"
        );
        assert!(
            *gap < expected + Duration::from_secs(2),
            "restart {i} waited {gap:?}, much longer than the expected {expected:?}"
        );
    }
}

#[tokio::test]
async fn restart_limit_stops_restarts_and_propagates_the_failure() {
    let (spec, spawns) = always_fails("budgeted", cfg(None, RestartBackoff::none()), None);
    let mut supervisor = spawn_supervisor(
        SupervisorBlueprint::one_for_one()
            .intensity(RestartIntensity::new(2, Duration::from_secs(60)))
            .child(spec),
    )
    .await;

    let error = tokio::time::timeout(TIMEOUT, &mut supervisor)
        .await
        .expect("supervisor should give up promptly")
        .expect_err("an exhausted restart budget should be the supervisor's own error");

    assert_eq!(
        spawns.load(Ordering::SeqCst),
        3,
        "the initial start plus exactly two restarts"
    );
    let error = error.to_string();
    assert!(
        error.contains("budget"),
        "the propagated error should explain the exhausted budget: {error}"
    );
    assert!(
        error.contains("budgeted"),
        "the propagated error should name the child: {error}"
    );
}

/// A child whose first generation initializes and then shuts down normally,
/// and whose every later generation fails before initializing. Under
/// rest-for-one, the first exit starts a cascade that restarts it; the
/// restarted child then fails initialization, which used to be retried in a
/// tight loop without ever consulting the supervisor-wide restart budget.
struct CrashThenFailInit {
    spawns: Arc<AtomicUsize>,
}

impl Actor for CrashThenFailInit {
    type Interface = ();
    type Exit = ();

    async fn run(self, mut inbox: Inbox<()>) -> Result<(), Report> {
        let generation = self.spawns.fetch_add(1, Ordering::SeqCst) + 1;

        if generation > 1 {
            return Err(rootcause::report!("cannot bind after restart"));
        }

        // First generation: `recv_event` auto-registers us as initialized,
        // then we exit normally when the test signals us.
        match inbox.recv_event().await {
            Some(InboxEvent::Signal(signal)) if signal.is_shutdown() => Ok(()),
            _ => Ok(()),
        }
    }
}

#[tokio::test]
async fn rest_for_one_cascade_retries_stop_at_the_supervisor_restart_limit() {
    let spawns = Arc::new(AtomicUsize::new(0));
    let for_actor = spawns.clone();

    let (spec, address) = fn_blueprint(move || CrashThenFailInit {
        spawns: for_actor.clone(),
    })
    .name("cascade")
    .expect("name is unique")
    .with_cfg(cfg(None, RestartBackoff::none()))
    .split();

    let mut supervisor = spawn_supervisor(
        SupervisorBlueprint::rest_for_one()
            .intensity(RestartIntensity::new(2, Duration::from_secs(60)))
            .child(spec),
    )
    .await;

    tokio::time::timeout(TIMEOUT, address.monitor_init())
        .await
        .expect("first generation should initialize")
        .expect("first generation should initialize");
    assert!(address.signal_shutdown(), "the child should accept signals");

    let result = tokio::time::timeout(TIMEOUT, &mut supervisor).await;
    assert!(
        result
            .expect("the cascade retry loop must not continue forever")
            .is_err(),
        "the supervisor should propagate its exhausted restart budget"
    );

    let spawns = spawns.load(Ordering::SeqCst);
    assert!(
        spawns <= 4,
        "rest-for-one retried a failing initialization {spawns} times despite a budget of 2"
    );
}

/// A child that reports itself `Running`, stays up for this generation's
/// configured time, then exits with an error. Generations past the configured
/// list fail immediately.
struct RunsThenFails {
    generation: Arc<AtomicUsize>,
    spawn_times: Arc<Mutex<Vec<tokio::time::Instant>>>,
    /// How long each generation stays `Running` before failing, indexed by
    /// generation.
    uptimes: Arc<Mutex<Vec<Duration>>>,
}

impl Actor for RunsThenFails {
    type Interface = ();
    type Exit = ();

    async fn run(self, mut inbox: Inbox<()>) -> Result<(), Report> {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst);
        self.spawn_times
            .lock()
            .unwrap()
            .push(tokio::time::Instant::now());

        // Register explicitly: this is the moment the supervisor sees the
        // child as `Running`, and the point the stable period is measured
        // from.
        inbox.register_initialized();

        let uptime = self
            .uptimes
            .lock()
            .unwrap()
            .get(generation)
            .copied()
            .unwrap_or(Duration::ZERO);

        if !uptime.is_zero() {
            tokio::time::sleep(uptime).await;
        }

        Err(rootcause::report!("simulated crash after {uptime:?}"))
    }
}

fn runs_then_fails(
    name: &str,
    cfg: ChildConfig,
    uptimes: Vec<Duration>,
) -> (
    ChildSpec,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<tokio::time::Instant>>>,
) {
    let name: Name = name.to_string().into();
    let spawns = Arc::new(AtomicUsize::new(0));
    let spawn_times = Arc::new(Mutex::new(Vec::new()));

    let for_actor = spawns.clone();
    let times_for_actor = spawn_times.clone();
    let uptimes = Arc::new(Mutex::new(uptimes));

    let (spec, _) = fn_blueprint(move || RunsThenFails {
        generation: for_actor.clone(),
        spawn_times: times_for_actor.clone(),
        uptimes: uptimes.clone(),
    })
    .name(name)
    .expect("name is unique")
    .with_cfg(cfg)
    .split();

    (spec, spawns, spawn_times)
}

fn spawn_gaps(spawn_times: &Arc<Mutex<Vec<tokio::time::Instant>>>) -> Vec<Duration> {
    let times = spawn_times.lock().unwrap();
    times.windows(2).map(|w| w[1] - w[0]).collect()
}

/// Each spawn's time relative to the first one, which is easier to read than
/// gaps when a generation stays up for a while between restarts.
fn spawn_offsets(spawn_times: &Arc<Mutex<Vec<tokio::time::Instant>>>) -> Vec<Duration> {
    let times = spawn_times.lock().unwrap();
    let Some((&first, rest)) = times.split_first() else {
        return Vec::new();
    };
    std::iter::once(Duration::ZERO)
        .chain(rest.iter().map(|time| *time - first))
        .collect()
}

/// A child that reaches `Running` and then fails immediately is not healthy:
/// every restart keeps the streak, so the delay doubles up to the cap.
#[tokio::test(start_paused = true)]
async fn reaching_running_then_failing_keeps_backing_off() {
    let (spec, spawns, spawn_times) = runs_then_fails(
        "running-then-failing",
        cfg(
            None,
            RestartBackoff::new(Duration::from_secs(1), Duration::from_secs(4)),
        ),
        Vec::new(),
    );

    // The initial start plus four restarts: delays 1, 2, 4, 4 s.
    let mut supervisor = spawn_supervisor(
        SupervisorBlueprint::one_for_one()
            .intensity(RestartIntensity::new(4, Duration::from_secs(60)))
            .child(spec),
    )
    .await;

    let result = tokio::time::timeout(PAUSED_TIMEOUT, &mut supervisor).await;
    assert!(
        result.expect("supervisor should give up promptly").is_err(),
        "the supervisor should propagate its exhausted restart budget"
    );
    assert_eq!(spawns.load(Ordering::SeqCst), 5);

    assert_eq!(
        spawn_gaps(&spawn_times),
        vec![
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(4),
        ]
    );
}

/// A child that stays `Running` past the stable period (here the default: the
/// backoff's maximum) has proven itself, so its next failure restarts at the
/// initial delay. A failure before that period continues the streak.
#[tokio::test(start_paused = true)]
async fn staying_running_for_the_stable_period_resets_the_backoff() {
    let (spec, spawns, spawn_times) = runs_then_fails(
        "stays-running",
        cfg(
            None,
            RestartBackoff::new(Duration::from_secs(1), Duration::from_secs(4)),
        ),
        vec![
            Duration::ZERO,
            // Longer than the 4 s stable period, so this generation's exit
            // resets the streak.
            Duration::from_secs(10),
            Duration::ZERO,
            Duration::ZERO,
        ],
    );

    let mut supervisor = spawn_supervisor(
        SupervisorBlueprint::one_for_one()
            .intensity(RestartIntensity::new(3, Duration::from_secs(60)))
            .child(spec),
    )
    .await;

    let result = tokio::time::timeout(PAUSED_TIMEOUT, &mut supervisor).await;
    assert!(
        result.expect("supervisor should give up promptly").is_err(),
        "the supervisor should propagate its exhausted restart budget"
    );
    assert_eq!(spawns.load(Ordering::SeqCst), 4);

    assert_eq!(
        spawn_offsets(&spawn_times),
        vec![
            Duration::ZERO,
            // Initial delay after the first generation failed immediately.
            Duration::from_secs(1),
            // Gen 2 stayed up past the stable period; its exit resets the
            // streak, so this restart waits the initial 1 s again, after gen
            // 2's own 10 s of uptime.
            Duration::from_secs(12),
            // Gen 3 failed immediately, so the streak continues and doubles.
            Duration::from_secs(14),
        ]
    );
}

/// `RestartBackoff::none()` keeps restarting as soon as the exit is observed,
/// however the child exited.
#[tokio::test(start_paused = true)]
async fn no_backoff_still_restarts_immediately() {
    let (spec, spawns, spawn_times) =
        runs_then_fails("no-backoff", cfg(None, RestartBackoff::none()), Vec::new());

    let mut supervisor = spawn_supervisor(
        SupervisorBlueprint::one_for_one()
            .intensity(RestartIntensity::new(3, Duration::from_secs(60)))
            .child(spec),
    )
    .await;

    let result = tokio::time::timeout(PAUSED_TIMEOUT, &mut supervisor).await;
    assert!(
        result.expect("supervisor should give up promptly").is_err(),
        "the supervisor should propagate its exhausted restart budget"
    );
    assert_eq!(spawns.load(Ordering::SeqCst), 4);
    assert_eq!(spawn_gaps(&spawn_times), vec![Duration::ZERO; 3]);
}
