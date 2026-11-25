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

/// Passes the future of every procedure, plain and with events, through [`require_send`].
macro_rules! require_every_procedure_send {
    ($link:expr, $params:expr, $video:expr, $record:expr) => {{
        let (link, params, video, record) = ($link, $params, $video, $record);
        let bring_up = BringUpConfig::DEFAULT;
        let tear_down = TearDownConfig::DEFAULT;
        require_send(link.bring_up(params, video, &bring_up));
        require_send(link.bring_up_with_events(params, video, &bring_up, &mut *record));
        require_send(link.tear_down(params, &tear_down, SinkPresence::Present));
        require_send(link.tear_down_with_events(
            params,
            &tear_down,
            SinkPresence::Present,
            &mut *record,
        ));
        require_send(link.check_reconnect(params, params.sink()));
        require_send(link.check_reconnect_with_events(params, params.sink(), &mut *record));
        require_send(link.quick_restore(params, &bring_up));
        require_send(link.quick_restore_with_events(params, &bring_up, &mut *record));
    }};
}

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
    require_every_procedure_send!(link, params, video, record);
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

/// 4K60 RGB 8 bpc, scrambled.
fn params() -> TmdsLinkParams {
    TmdsLinkParams::choose(
        594_000,
        SinkTmdsFacts::new(true, true, false),
        SourceTmdsFacts::from(&SendTmdsPhy::capabilities(&SendPhy)),
        TmdsPolicy::DEFAULT,
    )
    .unwrap()
}

fn video() -> TmdsVideo {
    TmdsVideo::new(594_000, ColorBitDepth::Depth8, ColorFormat::Rgb444)
}

#[test]
fn the_procedures_futures_are_send_over_the_twins() {
    let mut link = TmdsLink::new(SendSink, SendPhy, CellClock(Cell::new(0)), SendTimer)
        .with_cancel(CellFlag(Cell::new(false)));
    let mut events = 0_usize;
    every_procedure_is_send(&mut link, &params(), video(), &mut |_| events += 1);
    assert_eq!(events, 0, "nothing was polled");
}

// The culvert-async adapter.

/// Every procedure through the plain `Scdc<T>`, in code generic over the transport's twin:
/// the compiler sees through `Scdc<T>`'s implementation to the transport's `Send` futures.
#[cfg(feature = "culvert-async")]
fn every_procedure_is_send_through_the_client<T, P, K, D, X>(
    link: &mut TmdsLink<Scdc<T>, P, K, D, X>,
    params: &TmdsLinkParams,
    video: TmdsVideo,
    record: &mut (impl FnMut(TmdsEvent) + Send),
) where
    T: SendScdcTransport,
    P: SendTmdsPhy,
    K: Clock + Send,
    D: SendDelay,
    X: Cancel + Send,
{
    require_every_procedure_send!(link, params, video, record);
}

/// The same through a borrow of the client.
#[cfg(feature = "culvert-async")]
fn every_procedure_is_send_through_a_borrowed_client<T, P, K, D, X>(
    link: &mut TmdsLink<&mut Scdc<T>, P, K, D, X>,
    params: &TmdsLinkParams,
    video: TmdsVideo,
    record: &mut (impl FnMut(TmdsEvent) + Send),
) where
    T: SendScdcTransport,
    P: SendTmdsPhy,
    K: Clock + Send,
    D: SendDelay,
    X: Cancel + Send,
{
    require_every_procedure_send!(link, params, video, record);
}

/// A transport of the twin: `Sync`, as its `read(&self)` requires, so no `Cell`.
#[cfg(feature = "culvert-async")]
struct SendTransport;

#[cfg(feature = "culvert-async")]
impl SendScdcTransport for SendTransport {
    type Error = DdcErrorKind;

    async fn read(&self, _: u8) -> Result<u8, DdcErrorKind> {
        Ok(0)
    }

    async fn write(&mut self, _: u8, _: u8) -> Result<(), DdcErrorKind> {
        Ok(())
    }

    async fn read_block(&self, _: u8, buf: &mut [u8]) -> Result<(), DdcErrorKind> {
        buf.fill(0);
        Ok(())
    }
}

/// The local forwarding wrapper the docs describe for code bounded by `SendTmdsScdc`, which
/// takes no plain `Scdc<T>`: the twin over a `SendScdcTransport`, each method forwarded to
/// `Scdc<T>`'s implementation.
#[cfg(feature = "culvert-async")]
struct LocalSendScdc<T>(Scdc<T>);

#[cfg(feature = "culvert-async")]
impl<T: SendScdcTransport> SendTmdsScdc for LocalSendScdc<T> {
    type Error = CulvertError<T::Error>;

    async fn write_tmds_config(&mut self, c: SinkTmdsConfig) -> Result<(), Self::Error> {
        TmdsScdc::write_tmds_config(&mut self.0, c).await
    }

    async fn update_tmds_config(
        &mut self,
        c: SinkTmdsConfig,
    ) -> Result<(), UpdateError<Self::Error>> {
        TmdsScdc::update_tmds_config(&mut self.0, c).await
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, Self::Error> {
        TmdsScdc::read_tmds_config(&mut self.0).await
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, Self::Error> {
        TmdsScdc::read_scrambling_status(&mut self.0).await
    }

    async fn read_sink_version(&mut self) -> Result<u8, Self::Error> {
        TmdsScdc::read_sink_version(&mut self.0).await
    }

    async fn write_source_version(&mut self, version: u8) -> Result<(), Self::Error> {
        TmdsScdc::write_source_version(&mut self.0, version).await
    }

    async fn clear_frl_rate(&mut self) -> Result<(), Self::Error> {
        TmdsScdc::clear_frl_rate(&mut self.0).await
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<Self::Error>> {
        TmdsScdc::clear_flt_update(&mut self.0).await
    }
}

#[cfg(feature = "culvert-async")]
#[test]
fn the_adapters_futures_are_send_over_a_send_transport() {
    let clock = || CellClock(Cell::new(0));
    let flag = || CellFlag(Cell::new(false));
    let mut events = 0_usize;
    let mut record = |_: TmdsEvent| events += 1;

    let mut link =
        TmdsLink::new(Scdc::new(SendTransport), SendPhy, clock(), SendTimer).with_cancel(flag());
    every_procedure_is_send_through_the_client(&mut link, &params(), video(), &mut record);

    let mut client = Scdc::new(SendTransport);
    let mut link = TmdsLink::new(&mut client, SendPhy, clock(), SendTimer).with_cancel(flag());
    every_procedure_is_send_through_a_borrowed_client(&mut link, &params(), video(), &mut record);

    // Code bounded by `SendTmdsScdc` takes the client through a local forwarding wrapper.
    let scdc = LocalSendScdc(Scdc::new(SendTransport));
    let mut link = TmdsLink::new(scdc, SendPhy, clock(), SendTimer).with_cancel(flag());
    every_procedure_is_send(&mut link, &params(), video(), &mut record);

    assert_eq!(events, 0, "nothing was polled");
}
