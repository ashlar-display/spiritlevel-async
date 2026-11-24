//! Tests of the async driver.
//!
//! Scripted async doubles of the SCDC, the PHY and the clock-and-delay record every call in
//! one shared log, in the order the procedure makes them, and answer from a few registers.
//! Every other call returns `Pending` once before it completes, so the procedures run over
//! I/O that really suspends; [`run`] polls a procedure to completion and counts how often it
//! suspended.

extern crate std;

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::{Pin, pin};
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};
use core::time::Duration;
use std::vec::Vec;

use super::TmdsLink;
use crate::{
    BringUpConfig, BringUpOutcome, Cancel, CancelPoint, CheckResult, Cleanup, Clock, ClockRatio,
    ColorBitDepth, ColorFormat, DdcErrorKind, Delay, NoCancel, ReconnectCheck, RestoreError,
    ScramblingStatus, SinkClear, SinkPresence, SinkTmdsConfig, SinkTmdsFacts, SourceTmdsFacts,
    TearDownConfig, Timestamp, TmdsConfigReading, TmdsEvent, TmdsEventKind, TmdsLinkParams,
    TmdsPhy, TmdsPhyCapabilities, TmdsPhyConfig, TmdsPolicy, TmdsScdc, TmdsVideo, UpdateError,
};

/// Polls `future` to completion with a no-op waker, returning its output and how many times
/// it suspended.
fn run<F: Future>(future: F) -> (F::Output, usize) {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    let mut suspensions = 0;
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return (output, suspensions),
            Poll::Pending => {
                suspensions += 1;
                assert!(suspensions < 10_000, "the procedure never completed");
            }
        }
    }
}

/// Completes on its second poll, waking the task on its first unless `done` from the start.
struct YieldOnce {
    done: bool,
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.done {
            Poll::Ready(())
        } else {
            self.done = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// A call one of the doubles received, with its argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    WriteTmdsConfig(SinkTmdsConfig),
    UpdateTmdsConfig(SinkTmdsConfig),
    ReadTmdsConfig,
    ReadScramblingStatus,
    ReadSinkVersion,
    WriteSourceVersion(u8),
    ClearFrlRate,
    ClearFltUpdate,
    Configure(TmdsPhyConfig),
    SetScrambling(bool),
    StartOutput,
    StopOutput,
    Delay(Duration),
}

/// The calls of every double, in order.
#[derive(Default)]
struct Log {
    calls: RefCell<Vec<Call>>,
}

impl Log {
    /// Records `call`; odd-numbered calls suspend once before they complete.
    fn call(&self, call: Call) -> YieldOnce {
        let mut calls = self.calls.borrow_mut();
        calls.push(call);
        YieldOnce {
            done: calls.len() % 2 == 0,
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }
}

/// The sink's registers that matter here, with `TMDS_Config` as bits 0 (scrambling) and 1
/// (clock ratio).
struct Sink<'a> {
    log: &'a Log,
    tmds_config: u8,
    sink_version: u8,
    source_version: u8,
    flt_update: bool,
    scrambling_status: bool,
    /// Reads and writes of `TMDS_Config` fail with this.
    fail_tmds_config: Option<DdcErrorKind>,
}

impl<'a> Sink<'a> {
    fn new(log: &'a Log) -> Self {
        Self {
            log,
            tmds_config: 0,
            sink_version: 1,
            source_version: 0,
            flt_update: false,
            scrambling_status: false,
            fail_tmds_config: None,
        }
    }

    fn write(&mut self, config: SinkTmdsConfig) -> Result<(), DdcErrorKind> {
        if let Some(error) = self.fail_tmds_config {
            return Err(error);
        }
        self.tmds_config =
            u8::from(config.scrambling_enable) | (u8::from(config.high_tmds_clock_ratio) << 1);
        Ok(())
    }
}

impl TmdsScdc for Sink<'_> {
    type Error = DdcErrorKind;

    async fn write_tmds_config(&mut self, config: SinkTmdsConfig) -> Result<(), DdcErrorKind> {
        self.log.call(Call::WriteTmdsConfig(config)).await;
        self.write(config)
    }

    async fn update_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> Result<(), UpdateError<DdcErrorKind>> {
        self.log.call(Call::UpdateTmdsConfig(config)).await;
        self.write(config).map_err(UpdateError::Write)
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, DdcErrorKind> {
        self.log.call(Call::ReadTmdsConfig).await;
        if let Some(error) = self.fail_tmds_config {
            return Err(error);
        }
        let raw = self.tmds_config;
        Ok(TmdsConfigReading::new(
            SinkTmdsConfig::new(raw & 1 != 0, raw & 2 != 0),
            raw,
        ))
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, DdcErrorKind> {
        self.log.call(Call::ReadScramblingStatus).await;
        Ok(ScramblingStatus::new(
            self.scrambling_status,
            u8::from(self.scrambling_status),
        ))
    }

    async fn read_sink_version(&mut self) -> Result<u8, DdcErrorKind> {
        self.log.call(Call::ReadSinkVersion).await;
        Ok(self.sink_version)
    }

    async fn write_source_version(&mut self, version: u8) -> Result<(), DdcErrorKind> {
        self.log.call(Call::WriteSourceVersion(version)).await;
        self.source_version = version;
        Ok(())
    }

    async fn clear_frl_rate(&mut self) -> Result<(), DdcErrorKind> {
        self.log.call(Call::ClearFrlRate).await;
        Ok(())
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<DdcErrorKind>> {
        self.log.call(Call::ClearFltUpdate).await;
        Ok(core::mem::take(&mut self.flt_update))
    }
}

/// The PHY's error in these tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhyFault;

/// A PHY that tracks whether it transmits and scrambles.
struct Phy<'a> {
    log: &'a Log,
    output: bool,
    scrambling: bool,
    configured: Option<TmdsPhyConfig>,
    /// `stop_output` fails.
    fail_stop: bool,
}

impl<'a> Phy<'a> {
    fn new(log: &'a Log) -> Self {
        Self {
            log,
            output: false,
            scrambling: false,
            configured: None,
            fail_stop: false,
        }
    }
}

impl TmdsPhy for Phy<'_> {
    type Error = PhyFault;

    fn capabilities(&self) -> TmdsPhyCapabilities {
        TmdsPhyCapabilities::new(600_000).with_scrambling(true)
    }

    async fn configure(&mut self, config: TmdsPhyConfig) -> Result<(), PhyFault> {
        self.log.call(Call::Configure(config)).await;
        self.configured = Some(config);
        Ok(())
    }

    async fn set_scrambling(&mut self, enabled: bool) -> Result<(), PhyFault> {
        self.log.call(Call::SetScrambling(enabled)).await;
        self.scrambling = enabled;
        Ok(())
    }

    async fn start_output(&mut self) -> Result<(), PhyFault> {
        self.log.call(Call::StartOutput).await;
        self.output = true;
        Ok(())
    }

    async fn stop_output(&mut self) -> Result<(), PhyFault> {
        self.log.call(Call::StopOutput).await;
        if self.fail_stop {
            return Err(PhyFault);
        }
        self.output = false;
        Ok(())
    }
}

/// One time source as both clock and delay, shared by reference; the delay advances it.
struct SimTime<'a> {
    log: &'a Log,
    now: Cell<u64>,
    /// Set by the first delay, as a caller's task would while the procedure waits.
    cancel_on_delay: Option<&'a AtomicBool>,
}

impl<'a> SimTime<'a> {
    fn at(log: &'a Log, micros: u64) -> Self {
        Self {
            log,
            now: Cell::new(micros),
            cancel_on_delay: None,
        }
    }
}

impl Clock for &SimTime<'_> {
    fn now(&self) -> Timestamp {
        Timestamp::from_micros(self.now.get())
    }
}

impl Delay for &SimTime<'_> {
    async fn delay(&mut self, duration: Duration) {
        self.log.call(Call::Delay(duration)).await;
        let micros = u64::try_from(duration.as_micros()).unwrap();
        self.now.set(self.now.get() + micros);
        if let Some(flag) = self.cancel_on_delay {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// 4K60 RGB 8 bpc: scrambled at 1/40.
fn scrambled() -> TmdsLinkParams {
    TmdsLinkParams::choose(
        594_000,
        SinkTmdsFacts::new(true, true, false),
        SourceTmdsFacts::from(&Phy::new(&Log::default()).capabilities()),
        TmdsPolicy::DEFAULT,
    )
    .unwrap()
}

/// 1080p60 on a sink that does not declare low-rate scrambling: plain, at 1/10.
fn plain() -> TmdsLinkParams {
    TmdsLinkParams::choose(
        148_500,
        SinkTmdsFacts::new(true, true, false),
        SourceTmdsFacts::from(&Phy::new(&Log::default()).capabilities()),
        TmdsPolicy::DEFAULT,
    )
    .unwrap()
}

/// RGB 8 bpc video at `rate_khz`.
fn video(rate_khz: u32) -> TmdsVideo {
    TmdsVideo::new(rate_khz, ColorBitDepth::Depth8, ColorFormat::Rgb444)
}

// The parts between procedures.

#[test]
fn the_parts_come_back_as_given() {
    let log = Log::default();
    let time = SimTime::at(&log, 3);
    let mut sink = Sink::new(&log);
    sink.sink_version = 7;
    let mut phy = Phy::new(&log);
    phy.output = true;
    let link = TmdsLink::new(sink, phy, &time, &time);
    assert_eq!(link.scdc().sink_version, 7);
    assert!(link.phy().output);
    assert_eq!(link.clock().now(), Timestamp::from_micros(3));
    let (sink, phy, clock, delay, cancel) = link.into_parts();
    assert_eq!(sink.sink_version, 7);
    assert!(phy.output);
    assert!(core::ptr::eq(clock, &time) && core::ptr::eq(delay, &time));
    assert!(!cancel.is_cancelled());
    assert!(log.calls().is_empty(), "no I/O outside a procedure");
}

#[test]
fn the_mutable_accessors_reach_the_parts_the_driver_holds() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let mut link = TmdsLink::new(Sink::new(&log), Phy::new(&log), &time, &time);
    link.scdc_mut().source_version = 9;
    link.phy_mut().scrambling = true;
    let (sink, phy, ..) = link.into_parts();
    assert_eq!(sink.source_version, 9);
    assert!(phy.scrambling);
}

#[test]
fn with_cancel_replaces_only_the_flag() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let flag = AtomicBool::new(false);
    let mut sink = Sink::new(&log);
    sink.sink_version = 7;
    let link = TmdsLink::new(sink, Phy::new(&log), &time, &time).with_cancel(&flag);
    flag.store(true, Ordering::Relaxed);
    let (sink, _, clock, _, cancel) = link.into_parts();
    assert_eq!(sink.sink_version, 7);
    assert!(core::ptr::eq(clock, &time));
    assert!(core::ptr::eq(cancel, &flag), "the flag given, not a copy");
    assert!(cancel.is_cancelled(), "the driver sees the caller's store");

    // A second `with_cancel` replaces the first flag.
    let link = TmdsLink::new(Sink::new(&log), Phy::new(&log), &time, &time)
        .with_cancel(&flag)
        .with_cancel(NoCancel);
    assert!(!link.into_parts().4.is_cancelled());
}

// The tear-down.

#[test]
fn tear_down_clears_the_sink_and_stops_the_phy() {
    let log = Log::default();
    let time = SimTime::at(&log, 42);
    let mut sink = Sink::new(&log);
    sink.tmds_config = 0x03;
    let mut phy = Phy::new(&log);
    phy.output = true;
    phy.scrambling = true;
    let mut link = TmdsLink::new(sink, phy, &time, &time);

    let mut events: Vec<TmdsEvent> = Vec::new();
    let (result, suspensions) = run(link.tear_down_with_events(
        &scrambled(),
        &TearDownConfig::DEFAULT.with_clear_sink(true),
        SinkPresence::Present,
        &mut |event| events.push(event),
    ));

    assert_eq!(result.unwrap().sink, SinkClear::Cleared);
    assert!(suspensions > 0, "the procedure suspended on the I/O");
    assert_eq!(
        log.calls(),
        [
            Call::WriteTmdsConfig(SinkTmdsConfig::new(false, false)),
            Call::SetScrambling(false),
            Call::StopOutput,
        ]
    );
    assert_eq!(events.len(), crate::MAX_TEAR_DOWN_EVENTS);
    assert!(matches!(
        events.first().unwrap().kind,
        TmdsEventKind::TearDownStarted { .. }
    ));
    assert!(matches!(
        events.last().unwrap().kind,
        TmdsEventKind::TearDownEnded { .. }
    ));
    assert!(
        events.iter().all(|e| e.at == Timestamp::from_micros(42)),
        "stamped from the driver's clock"
    );

    let (sink, phy, ..) = link.into_parts();
    assert_eq!(sink.tmds_config, 0);
    assert!(!phy.output && !phy.scrambling);
}

#[test]
fn the_plain_tear_down_runs_whatever_the_cancel_flag() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let flag = AtomicBool::new(true);
    let mut sink = Sink::new(&log);
    sink.tmds_config = 0x03;
    let mut phy = Phy::new(&log);
    phy.output = true;
    let mut link = TmdsLink::new(sink, phy, &time, &time).with_cancel(&flag);

    let (result, _) =
        run(link.tear_down(&scrambled(), &TearDownConfig::DEFAULT, SinkPresence::Gone));

    assert_eq!(result.unwrap().sink, SinkClear::NotRequested);
    assert_eq!(log.calls(), [Call::StopOutput]);
    assert!(!link.phy().output);
    assert_eq!(link.scdc().tmds_config, 0x03, "the sink is not written");
}

#[test]
fn tear_down_passes_the_parts_errors_through() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let mut sink = Sink::new(&log);
    sink.tmds_config = 0x03;
    sink.fail_tmds_config = Some(DdcErrorKind::Detached);
    let mut phy = Phy::new(&log);
    phy.output = true;
    phy.fail_stop = true;
    let mut link = TmdsLink::new(sink, phy, &time, &time);

    let (result, _) = run(link.tear_down(
        &scrambled(),
        &TearDownConfig::DEFAULT.with_clear_sink(true),
        SinkPresence::Present,
    ));

    let error = result.unwrap_err();
    assert_eq!(error.sink, SinkClear::Failed(DdcErrorKind::Detached));
    assert_eq!(error.scrambler, None);
    assert_eq!(error.output, Some(PhyFault));
    assert_eq!(
        log.calls(),
        [
            Call::WriteTmdsConfig(SinkTmdsConfig::new(false, false)),
            Call::SetScrambling(false),
            Call::StopOutput,
        ],
        "every step attempted"
    );
}

// The bring-up.

#[test]
fn bring_up_on_a_scrambled_link() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let mut sink = Sink::new(&log);
    sink.scrambling_status = true;
    let mut link = TmdsLink::new(sink, Phy::new(&log), &time, &time);

    let mut events: Vec<TmdsEvent> = Vec::new();
    let (outcome, suspensions) = run(link.bring_up_with_events(
        &scrambled(),
        video(594_000),
        &BringUpConfig::DEFAULT,
        &mut |event| events.push(event),
    ));

    let Ok(BringUpOutcome::Up(up)) = outcome else {
        panic!("not up: {outcome:?}");
    };
    assert_eq!(up.link, scrambled());
    assert_eq!(up.check.result, CheckResult::Set);
    assert!(suspensions > 0, "the procedure suspended on the I/O");

    // The sink write, the PHY and the ratio wait all reached their parts.
    let calls = log.calls();
    assert!(calls.contains(&Call::UpdateTmdsConfig(SinkTmdsConfig::new(true, true))));
    assert!(calls.iter().any(|c| matches!(c, Call::Delay(_))));
    assert!(
        link.clock().now() > Timestamp::from_micros(0),
        "the delay advanced the clock"
    );
    assert!(calls.contains(&Call::StartOutput));
    assert!(calls.contains(&Call::ReadScramblingStatus));

    assert!(!events.is_empty() && events.len() <= crate::MAX_BRING_UP_EVENTS);
    assert!(
        events.windows(2).all(|w| w[0].at <= w[1].at),
        "stamped in order from the driver's clock"
    );

    let (sink, phy, ..) = link.into_parts();
    assert_eq!(sink.tmds_config, 0x03);
    assert_eq!(sink.source_version, 1);
    assert!(phy.output && phy.scrambling);
    assert_eq!(
        phy.configured.map(|c| c.clock_ratio),
        Some(ClockRatio::Div40)
    );
}

#[test]
fn the_plain_bring_up_on_a_plain_link() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let mut link = TmdsLink::new(Sink::new(&log), Phy::new(&log), &time, &time);

    // `Source_Version` only after reading `Sink_Version`, so that call is made too.
    let config =
        BringUpConfig::DEFAULT.with_source_version(crate::SourceVersionWrite::IfSinkVersion1);
    let (outcome, _) = run(link.bring_up(&plain(), video(148_500), &config));

    let Ok(BringUpOutcome::Up(up)) = outcome else {
        panic!("not up: {outcome:?}");
    };
    assert_eq!(up.check.result, CheckResult::NotApplicable);
    let calls = log.calls();
    assert!(!calls.contains(&Call::ReadScramblingStatus));
    assert!(calls.contains(&Call::ReadSinkVersion));
    let (sink, phy, ..) = link.into_parts();
    assert_eq!(sink.source_version, 1, "written after the sink reported 1");
    assert_eq!(sink.tmds_config & 0x03, 0, "neither bit set");
    assert!(phy.output && !phy.scrambling);
    assert_eq!(
        phy.configured.map(|c| c.clock_ratio),
        Some(ClockRatio::Div10)
    );
}

#[test]
fn a_flag_set_before_the_bring_up_stops_it_at_entry() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let flag = AtomicBool::new(true);
    let mut link = TmdsLink::new(Sink::new(&log), Phy::new(&log), &time, &time).with_cancel(&flag);

    let (outcome, _) = run(link.bring_up(&scrambled(), video(594_000), &BringUpConfig::DEFAULT));

    assert!(matches!(
        outcome,
        Ok(BringUpOutcome::Cancelled {
            at: CancelPoint::Entry,
            cleanup: Cleanup::Skipped,
            ..
        })
    ));
    assert!(log.calls().is_empty(), "nothing done");
    assert!(flag.load(Ordering::Relaxed), "the flag is not cleared");
}

#[test]
fn a_flag_set_during_the_bring_up_stops_it_after_its_tear_down() {
    let log = Log::default();
    let flag = AtomicBool::new(false);
    let mut time = SimTime::at(&log, 0);
    time.cancel_on_delay = Some(&flag);
    let mut sink = Sink::new(&log);
    sink.scrambling_status = true;
    let mut link = TmdsLink::new(sink, Phy::new(&log), &time, &time).with_cancel(&flag);

    let (outcome, _) = run(link.bring_up(&scrambled(), video(594_000), &BringUpConfig::DEFAULT));

    let Ok(BringUpOutcome::Cancelled { at, cleanup, .. }) = outcome else {
        panic!("not cancelled: {outcome:?}");
    };
    assert_ne!(at, CancelPoint::Entry, "seen after the wait began");
    assert!(matches!(cleanup, Cleanup::TornDown(Ok(_))));
    assert_eq!(
        log.calls().last(),
        Some(&Call::StopOutput),
        "torn down last"
    );
    assert!(!link.phy().output);
}

// The reconnect check.

#[test]
fn check_reconnect_reads_the_sink_once() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let mut sink = Sink::new(&log);
    sink.tmds_config = 0x03;
    let mut link = TmdsLink::new(sink, Phy::new(&log), &time, &time);
    let params = scrambled();

    let mut events: Vec<TmdsEvent> = Vec::new();
    let (result, suspensions) =
        run(link
            .check_reconnect_with_events(&params, params.sink(), &mut |event| events.push(event)));
    assert_eq!(result, ReconnectCheck::Matches);
    assert!(suspensions > 0, "the read suspended");
    assert_eq!(log.calls(), [Call::ReadTmdsConfig]);
    assert!(!events.is_empty() && events.len() <= crate::MAX_RECONNECT_CHECK_EVENTS);

    // The sink lost its bits: reported with the byte read, nothing written.
    link.scdc_mut().tmds_config = 0x00;
    let (result, _) = run(link.check_reconnect(&params, params.sink()));
    let ReconnectCheck::Lost { read, .. } = result else {
        panic!("not lost: {result:?}");
    };
    assert_eq!(read.raw, 0x00);
    assert_eq!(log.calls(), [Call::ReadTmdsConfig, Call::ReadTmdsConfig]);
}

#[test]
fn check_reconnect_on_a_changed_or_unreadable_sink() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let mut sink = Sink::new(&log);
    sink.fail_tmds_config = Some(DdcErrorKind::Detached);
    let mut link = TmdsLink::new(sink, Phy::new(&log), &time, &time);
    let params = scrambled();

    let (result, _) = run(link.check_reconnect(&params, SinkTmdsFacts::NONE));
    assert_eq!(result, ReconnectCheck::SinkChanged);
    assert!(log.calls().is_empty(), "nothing read");

    let (result, _) = run(link.check_reconnect(&params, params.sink()));
    assert_eq!(result, ReconnectCheck::Unreadable(DdcErrorKind::Detached));
}

// The quick restore.

/// A running scrambled link whose sink lost its `TMDS_Config` and whose source lost its
/// scrambler state.
fn running_after_a_loss(log: &Log) -> (Sink<'_>, Phy<'_>) {
    let mut sink = Sink::new(log);
    sink.scrambling_status = true;
    let mut phy = Phy::new(log);
    phy.output = true;
    (sink, phy)
}

#[test]
fn quick_restore_rewrites_the_sink_with_the_output_running() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let (sink, phy) = running_after_a_loss(&log);
    let mut link = TmdsLink::new(sink, phy, &time, &time);

    let mut events: Vec<TmdsEvent> = Vec::new();
    let (result, suspensions) = run(link.quick_restore_with_events(
        &scrambled(),
        &BringUpConfig::DEFAULT,
        &mut |event| events.push(event),
    ));

    let restored = result.unwrap();
    assert_eq!(restored.check.result, CheckResult::Set);
    assert!(restored.sink_written_at.is_some());
    assert!(suspensions > 0, "the procedure suspended on the I/O");
    assert!(events.len() <= crate::MAX_QUICK_RESTORE_EVENTS);
    let calls = log.calls();
    assert_eq!(
        calls[..2],
        [
            Call::UpdateTmdsConfig(SinkTmdsConfig::new(true, true)),
            Call::SetScrambling(true),
        ]
    );
    assert!(
        !calls
            .iter()
            .any(|c| matches!(c, Call::StopOutput | Call::Configure(_))),
        "the output keeps running at its rate"
    );
    let (sink, phy, ..) = link.into_parts();
    assert_eq!(sink.tmds_config, 0x03);
    assert!(phy.output && phy.scrambling);
}

#[test]
fn a_set_flag_stops_only_the_restores_check() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let flag = AtomicBool::new(true);
    let (sink, phy) = running_after_a_loss(&log);
    let mut link = TmdsLink::new(sink, phy, &time, &time).with_cancel(&flag);

    let (result, _) = run(link.quick_restore(&scrambled(), &BringUpConfig::DEFAULT));

    assert_eq!(result.unwrap().check.result, CheckResult::Cancelled);
    assert!(!log.calls().contains(&Call::ReadScramblingStatus));
    let (sink, phy, ..) = link.into_parts();
    assert_eq!(sink.tmds_config, 0x03, "the write is made");
    assert!(phy.output && phy.scrambling, "the link is left running");
}

#[test]
fn quick_restore_passes_the_sinks_error_through() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    let (mut sink, phy) = running_after_a_loss(&log);
    sink.fail_tmds_config = Some(DdcErrorKind::Detached);
    let mut link = TmdsLink::new(sink, phy, &time, &time);

    let (result, _) = run(link.quick_restore(&scrambled(), &BringUpConfig::DEFAULT));

    assert!(matches!(
        result,
        Err(RestoreError::Scdc(UpdateError::Write(
            DdcErrorKind::Detached
        )))
    ));
    assert_eq!(
        log.calls(),
        [Call::UpdateTmdsConfig(SinkTmdsConfig::new(true, true))],
        "nothing follows the failed write"
    );
    assert!(link.phy().output, "no tear-down: the link was running");
}

// The `Send` twin through the driver.

/// An SCDC implementation of the `Send` twin alone, answering `TMDS_Config` with scrambling
/// set and the ratio clear; the reconnect check makes no other call.
#[derive(Default)]
struct TwinSink {
    reads: u32,
}

impl crate::SendTmdsScdc for TwinSink {
    type Error = DdcErrorKind;

    async fn write_tmds_config(&mut self, _: SinkTmdsConfig) -> Result<(), DdcErrorKind> {
        unreachable!("the reconnect check only reads")
    }

    async fn update_tmds_config(
        &mut self,
        _: SinkTmdsConfig,
    ) -> Result<(), UpdateError<DdcErrorKind>> {
        unreachable!("the reconnect check only reads")
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, DdcErrorKind> {
        self.reads += 1;
        Ok(TmdsConfigReading::new(
            SinkTmdsConfig::new(true, false),
            0xF1,
        ))
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, DdcErrorKind> {
        unreachable!("the reconnect check reads only TMDS_Config")
    }

    async fn read_sink_version(&mut self) -> Result<u8, DdcErrorKind> {
        unreachable!("the reconnect check reads only TMDS_Config")
    }

    async fn write_source_version(&mut self, _: u8) -> Result<(), DdcErrorKind> {
        unreachable!("the reconnect check only reads")
    }

    async fn clear_frl_rate(&mut self) -> Result<(), DdcErrorKind> {
        unreachable!("the reconnect check only reads")
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<DdcErrorKind>> {
        unreachable!("the reconnect check only reads")
    }
}

#[test]
fn a_twin_implementation_drives_the_driver() {
    let log = Log::default();
    let time = SimTime::at(&log, 0);
    // `TwinSink` implements only the twin: the driver takes it as a `TmdsScdc` through the
    // blanket implementation.
    let mut link = TmdsLink::new(TwinSink::default(), Phy::new(&log), &time, &time);
    let params = scrambled();

    let (result, _) = run(link.check_reconnect(&params, params.sink()));

    // The sink holds scrambling without the ratio, not what this link needs.
    let ReconnectCheck::Lost { read, .. } = result else {
        panic!("not lost: {result:?}");
    };
    assert_eq!(read.raw, 0xF1, "the twin's reading, passed through");
    assert_eq!(link.scdc().reads, 1);
}
