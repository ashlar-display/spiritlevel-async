//! Sync and async bring-up agree: each procedure runs twice against maquette's simulated
//! sink on its simulated clock, once through spiritlevel's sync `TmdsLink` with spiritlevel's
//! culvert adapter and once through this crate's driver with the culvert-async adapter, each
//! on a fresh bench with the same sink settings and faults. The results, the event sequences
//! (timestamps included) and the sink's transcripts must be equal.

#![cfg(feature = "culvert-async")]
#![allow(
    clippy::result_large_err,
    reason = "the procedures' own result types, returned and compared as they are"
)]

use core::fmt::Debug;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use core::time::Duration;

use culvert::codec::TMDS_CONFIG;
use maquette::hdmi_hal::ddc::SCDC;
use maquette::hdmi_hal::scdc::ScdcOverDdc;
use maquette::hdmi_hal_async::scdc::SendScdcOverDdc;
use maquette::{
    Bench, BusError, Ddc, Effect, Fault, Filter, NoAcknowledgeSource, OpKind, Phy, PhyError,
    Record, SinkConfig, Time, Trigger,
};
use spiritlevel_async::{
    BringUpConfig, BringUpError, BringUpOutcome, BringUpWarning, Cancel, CancelPoint, CheckResult,
    Clock, ColorBitDepth, ColorFormat, CulvertError, DdcError, DdcErrorKind, ReconnectCheck,
    RestoreError, Restored, SinkClear, SinkPresence, SinkTmdsFacts, SourceTmdsFacts,
    TearDownConfig, TearDownError, Timestamp, TmdsEvent, TmdsLinkParams, TmdsPolicy, TmdsVideo,
    TornDown, UpdateError,
};

/// Room for every record of the longest case, a check that runs its whole window.
const LOG: usize = 4096;

type TestBench = Bench<'static, LOG, 16>;

type ScdcError = CulvertError<BusError>;
type BringUpResult = Result<BringUpOutcome<ScdcError, PhyError>, BringUpError<ScdcError, PhyError>>;
type TearDownResult = Result<TornDown<ScdcError>, TearDownError<ScdcError, PhyError>>;
type RestoreResult = Result<Restored<ScdcError>, RestoreError<ScdcError, PhyError>>;

/// A cancel flag set from a moment on the bench's clock, so both drivers see it set at the
/// same point of the same procedure.
#[derive(Clone, Copy)]
struct CancelAt<'b> {
    time: Time<'b>,
    at: Option<Timestamp>,
}

impl Cancel for CancelAt<'_> {
    fn is_cancelled(&self) -> bool {
        self.at.is_some_and(|at| self.time.now() >= at)
    }
}

/// The procedures, as both drivers run them, each recording its events into `events`.
trait Driver {
    fn bring_up(
        &mut self,
        link: &TmdsLinkParams,
        video: TmdsVideo,
        config: &BringUpConfig,
        events: &mut Vec<TmdsEvent>,
    ) -> BringUpResult;

    fn tear_down(
        &mut self,
        link: &TmdsLinkParams,
        config: &TearDownConfig,
        presence: SinkPresence,
        events: &mut Vec<TmdsEvent>,
    ) -> TearDownResult;

    fn check_reconnect(
        &mut self,
        link: &TmdsLinkParams,
        current_sink: SinkTmdsFacts,
        events: &mut Vec<TmdsEvent>,
    ) -> ReconnectCheck<ScdcError>;

    fn quick_restore(
        &mut self,
        link: &TmdsLinkParams,
        config: &BringUpConfig,
        events: &mut Vec<TmdsEvent>,
    ) -> RestoreResult;
}

type SyncLink<'b> = spiritlevel::TmdsLink<
    spiritlevel::Scdc<ScdcOverDdc<Ddc<'b>>>,
    Phy<'b>,
    Time<'b>,
    Time<'b>,
    CancelAt<'b>,
>;

impl Driver for SyncLink<'_> {
    fn bring_up(
        &mut self,
        link: &TmdsLinkParams,
        video: TmdsVideo,
        config: &BringUpConfig,
        events: &mut Vec<TmdsEvent>,
    ) -> BringUpResult {
        self.bring_up_with_events(link, video, config, &mut |e| events.push(e))
    }

    fn tear_down(
        &mut self,
        link: &TmdsLinkParams,
        config: &TearDownConfig,
        presence: SinkPresence,
        events: &mut Vec<TmdsEvent>,
    ) -> TearDownResult {
        self.tear_down_with_events(link, config, presence, &mut |e| events.push(e))
    }

    fn check_reconnect(
        &mut self,
        link: &TmdsLinkParams,
        current_sink: SinkTmdsFacts,
        events: &mut Vec<TmdsEvent>,
    ) -> ReconnectCheck<ScdcError> {
        self.check_reconnect_with_events(link, current_sink, &mut |e| events.push(e))
    }

    fn quick_restore(
        &mut self,
        link: &TmdsLinkParams,
        config: &BringUpConfig,
        events: &mut Vec<TmdsEvent>,
    ) -> RestoreResult {
        self.quick_restore_with_events(link, config, &mut |e| events.push(e))
    }
}

type AsyncLink<'b> = spiritlevel_async::TmdsLink<
    spiritlevel_async::Scdc<SendScdcOverDdc<Ddc<'b>>>,
    Phy<'b>,
    Time<'b>,
    Time<'b>,
    CancelAt<'b>,
>;

/// Polls `future` once: the bench is synchronous, so each of its async calls is ready when
/// first polled, and so is a procedure over them.
fn block_on<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the bench's futures are ready when first polled"),
    }
}

impl Driver for AsyncLink<'_> {
    fn bring_up(
        &mut self,
        link: &TmdsLinkParams,
        video: TmdsVideo,
        config: &BringUpConfig,
        events: &mut Vec<TmdsEvent>,
    ) -> BringUpResult {
        block_on(self.bring_up_with_events(link, video, config, &mut |e| events.push(e)))
    }

    fn tear_down(
        &mut self,
        link: &TmdsLinkParams,
        config: &TearDownConfig,
        presence: SinkPresence,
        events: &mut Vec<TmdsEvent>,
    ) -> TearDownResult {
        block_on(self.tear_down_with_events(link, config, presence, &mut |e| events.push(e)))
    }

    fn check_reconnect(
        &mut self,
        link: &TmdsLinkParams,
        current_sink: SinkTmdsFacts,
        events: &mut Vec<TmdsEvent>,
    ) -> ReconnectCheck<ScdcError> {
        block_on(self.check_reconnect_with_events(link, current_sink, &mut |e| events.push(e)))
    }

    fn quick_restore(
        &mut self,
        link: &TmdsLinkParams,
        config: &BringUpConfig,
        events: &mut Vec<TmdsEvent>,
    ) -> RestoreResult {
        block_on(self.quick_restore_with_events(link, config, &mut |e| events.push(e)))
    }
}

fn sync_driver<'b>(bench: &'b TestBench, cancel: CancelAt<'b>) -> Box<dyn Driver + 'b> {
    let scdc = spiritlevel::Scdc::new(ScdcOverDdc::new(bench.ddc()).ok().unwrap());
    let link = spiritlevel::TmdsLink::new(scdc, bench.phy(), bench.time(), bench.time());
    Box::new(link.with_cancel(cancel))
}

fn async_driver<'b>(bench: &'b TestBench, cancel: CancelAt<'b>) -> Box<dyn Driver + 'b> {
    let scdc = spiritlevel_async::Scdc::new(SendScdcOverDdc::new(bench.ddc()).ok().unwrap());
    let link = spiritlevel_async::TmdsLink::new(scdc, bench.phy(), bench.time(), bench.time());
    Box::new(link.with_cancel(cancel))
}

/// What one driver did on its bench: the script's result, the events, the transcript.
#[derive(Debug, PartialEq)]
struct Run<R> {
    result: R,
    events: Vec<TmdsEvent>,
    transcript: Vec<Record>,
}

type MakeDriver = for<'b> fn(&'b TestBench, CancelAt<'b>) -> Box<dyn Driver + 'b>;

/// Runs `script` through one driver on a fresh bench with `sink`, after `prepare`, with the
/// cancel flag set `cancel_after` into the script (never when `None`).
fn run<R>(
    make: MakeDriver,
    sink: SinkConfig,
    prepare: fn(&TestBench),
    cancel_after: Option<Duration>,
    script: &impl Fn(&mut dyn Driver, &mut Vec<TmdsEvent>, &TestBench) -> R,
) -> Run<R> {
    let bench: TestBench = Bench::builder(&[])
        .sink(sink)
        .capacities::<LOG, 16>()
        .build()
        .unwrap();
    prepare(&bench);
    let cancel = CancelAt {
        time: bench.time(),
        at: cancel_after.map(|after| bench.now().checked_add(after).unwrap()),
    };
    let mut events = Vec::new();
    let result = {
        let mut driver = make(&bench, cancel);
        script(&mut *driver, &mut events, &bench)
    };
    assert_eq!(
        bench.transcript_overflow(),
        0,
        "the transcript kept every record"
    );
    Run {
        result,
        events,
        transcript: bench.transcript().collect(),
    }
}

/// Runs `script` through both drivers and asserts they agree; returns the result.
fn agree<R: PartialEq + Debug>(
    sink: SinkConfig,
    prepare: fn(&TestBench),
    cancel_after: Option<Duration>,
    script: impl Fn(&mut dyn Driver, &mut Vec<TmdsEvent>, &TestBench) -> R,
) -> R {
    let sync = run(sync_driver, sink, prepare, cancel_after, &script);
    let asynch = run(async_driver, sink, prepare, cancel_after, &script);
    assert!(!sync.events.is_empty(), "the procedure recorded events");
    assert!(
        !sync.transcript.is_empty(),
        "the bench recorded the procedure"
    );
    assert_eq!(sync.result, asynch.result, "the results differ");
    assert_eq!(sync.events, asynch.events, "the events differ");
    assert_eq!(sync.transcript, asynch.transcript, "the transcripts differ");
    sync.result
}

fn nothing(_: &TestBench) {}

/// The link for `rate_khz` on a sink with SCDC and scrambling, without low-rate scrambling,
/// from the bench PHY's capabilities.
fn link(rate_khz: u32) -> TmdsLinkParams {
    let bench = Bench::new(SinkConfig::new(), &[]).unwrap();
    let capabilities = maquette::TmdsPhy::capabilities(&bench.phy());
    TmdsLinkParams::choose(
        rate_khz,
        SinkTmdsFacts::new(true, true, false),
        SourceTmdsFacts::from(&capabilities),
        TmdsPolicy::DEFAULT,
    )
    .unwrap()
}

fn video(rate_khz: u32) -> TmdsVideo {
    TmdsVideo::new(rate_khz, ColorBitDepth::Depth8, ColorFormat::Rgb444)
}

/// 4K60: scrambled at 1/40.
const SCRAMBLED: u32 = 594_000;
/// 1080p60: plain at 1/10.
const PLAIN: u32 = 148_500;

/// A bring-up with the default settings.
fn bring_up(
    rate_khz: u32,
) -> impl Fn(&mut dyn Driver, &mut Vec<TmdsEvent>, &TestBench) -> BringUpResult {
    move |driver, events, _| {
        driver.bring_up(
            &link(rate_khz),
            video(rate_khz),
            &BringUpConfig::DEFAULT,
            events,
        )
    }
}

/// The scrambling check of an outcome that came up.
fn check(outcome: &BringUpResult) -> CheckResult {
    match outcome {
        Ok(BringUpOutcome::Up(up)) => up.check.result,
        other => panic!("not up: {other:?}"),
    }
}

#[test]
fn a_scrambled_bring_up() {
    let outcome = agree(SinkConfig::new(), nothing, None, bring_up(SCRAMBLED));
    assert_eq!(check(&outcome), CheckResult::Set);
}

#[test]
fn a_plain_bring_up() {
    let outcome = agree(SinkConfig::new(), nothing, None, bring_up(PLAIN));
    assert_eq!(check(&outcome), CheckResult::NotApplicable);
}

#[test]
fn a_sink_that_reports_scrambling_late() {
    // After the default deadline (100 ms), inside the check's window (200 ms).
    let late = SinkConfig::new().with_scrambling_lock(Trigger::After(Duration::from_millis(150)));
    let outcome = agree(late, nothing, None, bring_up(SCRAMBLED));
    assert_eq!(check(&outcome), CheckResult::Set);
}

#[test]
fn a_sink_that_never_reports_scrambling() {
    let never = SinkConfig::new().with_scrambling_lock(Trigger::After(Duration::from_secs(3600)));
    let outcome = agree(never, nothing, None, bring_up(SCRAMBLED));
    assert_eq!(check(&outcome), CheckResult::NotSet);
}

#[test]
fn a_failed_tmds_config_write() {
    const NACK: DdcErrorKind = DdcErrorKind::NoAcknowledge(NoAcknowledgeSource::Data);
    fn fail_the_write(bench: &TestBench) {
        // The write of the read-modify-write, not its read: a write operation addressed to
        // `TMDS_Config`.
        let filter = Filter::ANY
            .with_addr(SCDC)
            .with_first(TMDS_CONFIG)
            .with_op(OpKind::Write);
        bench
            .schedule(Fault::new(Effect::Fail(NACK)).on(filter))
            .unwrap();
    }
    let outcome = agree(SinkConfig::new(), fail_the_write, None, bring_up(SCRAMBLED));
    let Ok(BringUpOutcome::Up(up)) = &outcome else {
        panic!("not up: {outcome:?}");
    };
    assert!(
        up.warnings.iter_warnings().any(|warning| matches!(
            warning,
            BringUpWarning::TmdsConfigWriteFailed { error: UpdateError::Write(e), .. }
                if e.kind() == NACK
        )),
        "the write half failed with the fault's error: {:?}",
        up.warnings
    );
}

#[test]
fn a_detached_sink() {
    fn detach(bench: &TestBench) {
        bench.detach();
    }
    let outcome = agree(SinkConfig::new(), detach, None, bring_up(SCRAMBLED));
    assert!(
        outcome.is_err(),
        "a detached sink stops the bring-up: {outcome:?}"
    );
}

#[test]
fn a_cancelled_bring_up() {
    // Set half a millisecond in: after entry, before the bring-up ends.
    let after = Some(Duration::from_micros(500));
    let outcome = agree(SinkConfig::new(), nothing, after, bring_up(SCRAMBLED));
    let Ok(BringUpOutcome::Cancelled { at, .. }) = outcome else {
        panic!("not cancelled: {outcome:?}");
    };
    assert_ne!(at, CancelPoint::Entry, "seen during the bring-up");
}

#[test]
fn a_tear_down_after_a_bring_up() {
    let (up, down) = agree(SinkConfig::new(), nothing, None, |driver, events, _| {
        let link = link(SCRAMBLED);
        let up = driver.bring_up(&link, video(SCRAMBLED), &BringUpConfig::DEFAULT, events);
        let config = TearDownConfig::DEFAULT.with_clear_sink(true);
        let down = driver.tear_down(&link, &config, SinkPresence::Present, events);
        (up, down)
    });
    assert_eq!(check(&up), CheckResult::Set);
    assert_eq!(down.unwrap().sink, SinkClear::Cleared);
}

#[test]
fn the_reconnect_check_and_the_quick_restore() {
    let (matches, lost, restored) =
        agree(SinkConfig::new(), nothing, None, |driver, events, bench| {
            let link = link(SCRAMBLED);
            let config = BringUpConfig::DEFAULT;
            check(&driver.bring_up(&link, video(SCRAMBLED), &config, events));
            let matches = driver.check_reconnect(&link, link.sink(), events);
            // The sink loses its TMDS_Config, as after a hotplug.
            bench.preset_scdc(TMDS_CONFIG, 0);
            let lost = driver.check_reconnect(&link, link.sink(), events);
            let restored = driver.quick_restore(&link, &config, events);
            (matches, lost, restored)
        });
    assert_eq!(matches, ReconnectCheck::Matches);
    assert!(matches!(lost, ReconnectCheck::Lost { .. }), "{lost:?}");
    assert_eq!(restored.unwrap().check.result, CheckResult::Set);
}
