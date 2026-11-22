//! Compile-time checks: the procedures' futures are `Send` in code generic over the `Send`
//! twins, and every re-export is reachable through the crate root.
//!
//! Nothing here is polled: each future is built, passed through [`require_send`] and dropped.
//! The checks are the type checker's, so a change that breaks one fails to compile.

use core::cell::Cell;
use core::time::Duration;

#[allow(
    unused_imports,
    reason = "each name is imported to prove the re-export exists; not all are used"
)]
use spiritlevel_async::{
    // the settings
    BringUpConfig,
    // the outcomes and errors
    BringUpError,
    BringUpOutcome,
    BringUpWarning,
    // hdmi-hal-async's
    Cancel,
    // the events
    CancelPoint,
    CheckResult,
    Cleanup,
    ClearFrl,
    Clock,
    // the link parameters
    ClockRatio,
    ColorBitDepth,
    ColorFormat,
    ConfigError,
    DdcError,
    DdcErrorKind,
    Delay,
    FailedRead,
    FrlClearSkip,
    LinkUp,
    MAX_BRING_UP_EVENTS,
    MAX_QUICK_RESTORE_EVENTS,
    MAX_RECONNECT_CHECK_EVENTS,
    MAX_TEAR_DOWN_EVENTS,
    NoAcknowledgeSource,
    NoCancel,
    OnTmdsConfigFailure,
    OutputDuringSinkWrite,
    OutputState,
    ParamsError,
    ReconnectCheck,
    RestoreError,
    Restored,
    ScramblerOff,
    ScramblingCheck,
    ScramblingDecision,
    ScramblingOff,
    // the SCDC values
    ScramblingStatus,
    ScramblingThreshold,
    SendDelay,
    SendTmdsPhy,
    // the driver and its SCDC traits
    SendTmdsScdc,
    SinkClear,
    SinkClearSkip,
    SinkClearTiming,
    SinkPresence,
    SinkTmdsConfig,
    SinkTmdsFacts,
    SourceTmdsFacts,
    SourceVersionSkip,
    SourceVersionWrite,
    TearDownConfig,
    TearDownError,
    Timestamp,
    TmdsConfigGate,
    TmdsConfigReading,
    TmdsConfigSkip,
    TmdsConfigWrite,
    TmdsEvent,
    TmdsEventKind,
    TmdsLink,
    TmdsLinkParams,
    TmdsPhy,
    TmdsPhyCapabilities,
    TmdsPhyConfig,
    TmdsPolicy,
    TmdsScdc,
    TmdsStep,
    TmdsVideo,
    TornDown,
    UpdateError,
    Warnings,
};

#[cfg(feature = "display-types")]
#[allow(unused_imports, reason = "imported to prove the re-export exists")]
use spiritlevel_async::HdmiForumSinkCap;

#[cfg(feature = "culvert-async")]
#[allow(unused_imports, reason = "imported to prove the re-exports exist")]
use spiritlevel_async::{
    CulvertError, ProtocolError, Scdc, ScdcError, ScdcTransport, SendScdcTransport,
};

/// Accepts only a value that is `Send`, as `tokio::spawn` does a future.
fn require_send<T: Send>(_: T) {}

/// Every procedure's future, plain and with events, over parts bounded by the twins alone,
/// with a clock and a flag that are `Send` but not required to be `Sync`.
fn every_procedure_is_send<S, P, K, D, X>(
    link: &mut TmdsLink<S, P, K, D, X>,
    params: &TmdsLinkParams,
    video: TmdsVideo,
    record: &mut (impl FnMut(TmdsEvent) + Send),
) where
    S: SendTmdsScdc,
    P: SendTmdsPhy,
    K: Clock + Send,
    D: SendDelay,
    X: Cancel + Send,
{
    let bring_up = BringUpConfig::DEFAULT;
    let tear_down = TearDownConfig::DEFAULT;

    require_send(link.bring_up(params, video, &bring_up));
    require_send(link.bring_up_with_events(params, video, &bring_up, record));
    require_send(link.tear_down(params, &tear_down, SinkPresence::Present));
    require_send(link.tear_down_with_events(params, &tear_down, SinkPresence::Present, record));
    require_send(link.check_reconnect(params, params.sink()));
    require_send(link.check_reconnect_with_events(params, params.sink(), record));
    require_send(link.quick_restore(params, &bring_up));
    require_send(link.quick_restore_with_events(params, &bring_up, record));
}

/// An SCDC implementation of the twin, answering with fixed values.
struct SendSink;

impl SendTmdsScdc for SendSink {
    type Error = DdcErrorKind;

    async fn write_tmds_config(&mut self, _: SinkTmdsConfig) -> Result<(), DdcErrorKind> {
        Ok(())
    }

    async fn update_tmds_config(
        &mut self,
        _: SinkTmdsConfig,
    ) -> Result<(), UpdateError<DdcErrorKind>> {
        Ok(())
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, DdcErrorKind> {
        Ok(TmdsConfigReading::new(
            SinkTmdsConfig::new(true, true),
            0x03,
        ))
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, DdcErrorKind> {
        Ok(ScramblingStatus::new(true, 0x01))
    }

    async fn read_sink_version(&mut self) -> Result<u8, DdcErrorKind> {
        Ok(1)
    }

    async fn write_source_version(&mut self, _: u8) -> Result<(), DdcErrorKind> {
        Ok(())
    }

    async fn clear_frl_rate(&mut self) -> Result<(), DdcErrorKind> {
        Ok(())
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<DdcErrorKind>> {
        Ok(false)
    }
}

/// A PHY implementation of the twin.
struct SendPhy;

impl SendTmdsPhy for SendPhy {
    type Error = ();

    fn capabilities(&self) -> TmdsPhyCapabilities {
        TmdsPhyCapabilities::new(600_000).with_scrambling(true)
    }

    async fn configure(&mut self, _: TmdsPhyConfig) -> Result<(), ()> {
        Ok(())
    }

    async fn set_scrambling(&mut self, _: bool) -> Result<(), ()> {
        Ok(())
    }

    async fn start_output(&mut self) -> Result<(), ()> {
        Ok(())
    }

    async fn stop_output(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

/// A delay implementation of the twin.
struct SendTimer;

impl SendDelay for SendTimer {
    async fn delay(&mut self, _: Duration) {}
}

/// A clock that is `Send` but not `Sync`: a `Cell`.
struct CellClock(Cell<u64>);

impl Clock for CellClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_micros(self.0.get())
    }
}

/// A cancel flag that is `Send` but not `Sync`: a `Cell`.
struct CellFlag(Cell<bool>);

impl Cancel for CellFlag {
    fn is_cancelled(&self) -> bool {
        self.0.get()
    }
}

#[test]
fn the_procedures_futures_are_send_over_the_twins() {
    let mut link = TmdsLink::new(SendSink, SendPhy, CellClock(Cell::new(0)), SendTimer)
        .with_cancel(CellFlag(Cell::new(false)));
    let params = TmdsLinkParams::choose(
        594_000,
        SinkTmdsFacts::new(true, true, false),
        SourceTmdsFacts::from(&SendTmdsPhy::capabilities(&SendPhy)),
        TmdsPolicy::DEFAULT,
    )
    .unwrap();
    let video = TmdsVideo::new(594_000, ColorBitDepth::Depth8, ColorFormat::Rgb444);
    let mut events = 0_usize;
    every_procedure_is_send(&mut link, &params, video, &mut |_| events += 1);
    assert_eq!(events, 0, "nothing was polled");
}
