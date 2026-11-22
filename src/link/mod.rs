//! The async driver: [`TmdsLink`] runs spiritlevel's procedures over an async [`TmdsScdc`],
//! an async `TmdsPhy`, a `Clock` and an async `Delay`, with an optional [`Cancel`] flag.

use core::time::Duration;

use spiritlevel::procedure::{self, TmdsIo};

use crate::{
    BringUpConfig, BringUpError, BringUpOutcome, Cancel, Clock, Delay, NoCancel, ScramblingStatus,
    SinkPresence, SinkTmdsConfig, TearDownConfig, TearDownError, Timestamp, TmdsConfigReading,
    TmdsEvent, TmdsLinkParams, TmdsPhy, TmdsPhyConfig, TmdsScdc, TmdsVideo, TornDown, UpdateError,
};

/// What a bring-up returns, over the parts' error types.
type BringUpResult<S, P> = Result<
    BringUpOutcome<<S as TmdsScdc>::Error, <P as TmdsPhy>::Error>,
    BringUpError<<S as TmdsScdc>::Error, <P as TmdsPhy>::Error>,
>;

/// What a tear-down returns, over the parts' error types.
type TearDownResult<S, P> = Result<
    TornDown<<S as TmdsScdc>::Error>,
    TearDownError<<S as TmdsScdc>::Error, <P as TmdsPhy>::Error>,
>;

#[cfg(test)]
mod tests;

/// The TMDS procedures over async parts: the sink's SCDC, the PHY, a clock and a delay, and a
/// cancel flag ([`NoCancel`] unless [`with_cancel`](Self::with_cancel) gives one).
///
/// The API is `spiritlevel::TmdsLink`'s, with the procedures `async`: each `.await`s the
/// parts' calls and the delay, so a procedure yields to the executor wherever the I/O or a
/// wait does. The driver keeps no state between procedures.
///
/// One clock-and-delay value can serve as both: implement `Clock` and `Delay` for a reference
/// to it, and pass the reference twice. For a `Send` future the shared value must then be
/// `Sync` (an atomic rather than a `Cell`), or the clock and the delay kept separate.
#[derive(Debug)]
pub struct TmdsLink<S, P, K, D, X = NoCancel> {
    scdc: S,
    phy: P,
    clock: K,
    delay: D,
    cancel: X,
}

impl<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay> TmdsLink<S, P, K, D, NoCancel> {
    /// The driver over its parts, never cancelled.
    pub fn new(scdc: S, phy: P, clock: K, delay: D) -> Self {
        Self {
            scdc,
            phy,
            clock,
            delay,
            cancel: NoCancel,
        }
    }
}

impl<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay, X: Cancel> TmdsLink<S, P, K, D, X> {
    /// The same driver with `cancel` as its cancel flag.
    ///
    /// A flag shared with another task is passed by reference, as `&AtomicBool`: [`Cancel`]
    /// is implemented for the `AtomicBool` and references to it, not for an `Arc` of one.
    pub fn with_cancel<X2: Cancel>(self, cancel: X2) -> TmdsLink<S, P, K, D, X2> {
        TmdsLink {
            scdc: self.scdc,
            phy: self.phy,
            clock: self.clock,
            delay: self.delay,
            cancel,
        }
    }

    /// The parts back.
    pub fn into_parts(self) -> (S, P, K, D, X) {
        (self.scdc, self.phy, self.clock, self.delay, self.cancel)
    }

    /// The SCDC client.
    pub fn scdc(&self) -> &S {
        &self.scdc
    }

    /// The SCDC client, mutably: between procedures, for reads of the caller's own.
    pub fn scdc_mut(&mut self) -> &mut S {
        &mut self.scdc
    }

    /// The PHY.
    pub fn phy(&self) -> &P {
        &self.phy
    }

    /// The PHY, mutably: between procedures, to power it on or off, read its capabilities, or
    /// drive it in another mode.
    pub fn phy_mut(&mut self) -> &mut P {
        &mut self.phy
    }

    /// The clock the procedures stamp their events from, so a caller can stamp its own on
    /// the same timebase.
    pub fn clock(&self) -> &K {
        &self.clock
    }

    /// Stops the TMDS link `link` describes; see [`procedure::tear_down`].
    ///
    /// Not stopped by the cancel flag: tear-down is the recovery. If its future is dropped
    /// before it completes, call it again.
    ///
    /// # Errors
    ///
    /// [`TearDownError`] when a PHY step failed; every step was still attempted.
    pub async fn tear_down(
        &mut self,
        link: &TmdsLinkParams,
        config: &TearDownConfig,
        presence: SinkPresence,
    ) -> TearDownResult<S, P> {
        self.tear_down_with_events(link, config, presence, &mut |_| {})
            .await
    }

    /// [`tear_down`](Self::tear_down), calling `record` with each event, at most
    /// [`MAX_TEAR_DOWN_EVENTS`](crate::MAX_TEAR_DOWN_EVENTS).
    ///
    /// # Errors
    ///
    /// As [`tear_down`](Self::tear_down).
    pub async fn tear_down_with_events(
        &mut self,
        link: &TmdsLinkParams,
        config: &TearDownConfig,
        presence: SinkPresence,
        record: &mut impl FnMut(TmdsEvent),
    ) -> TearDownResult<S, P> {
        procedure::tear_down(&mut AsyncIo(self), link, config, presence, record).await
    }

    /// Brings up the TMDS link `link` describes, carrying `video`; see
    /// [`procedure::bring_up`], which says where the cancel flag is read and what each
    /// outcome leaves.
    ///
    /// Stop it through the cancel flag, never by dropping its future: a dropped bring-up
    /// stops at whichever `.await` it was suspended on, with no tear-down, and may leave the
    /// sink holding a new `TMDS_Config` and the PHY configured or transmitting. After one,
    /// call [`tear_down`](Self::tear_down) with the same `link` before anything else.
    ///
    /// # Errors
    ///
    /// [`BringUpError`] when a step failed in a way that stops the bring-up.
    pub async fn bring_up(
        &mut self,
        link: &TmdsLinkParams,
        video: TmdsVideo,
        config: &BringUpConfig,
    ) -> BringUpResult<S, P> {
        self.bring_up_with_events(link, video, config, &mut |_| {})
            .await
    }

    /// [`bring_up`](Self::bring_up), calling `record` with each event, at most
    /// [`MAX_BRING_UP_EVENTS`](crate::MAX_BRING_UP_EVENTS).
    ///
    /// # Errors
    ///
    /// As [`bring_up`](Self::bring_up).
    pub async fn bring_up_with_events(
        &mut self,
        link: &TmdsLinkParams,
        video: TmdsVideo,
        config: &BringUpConfig,
        record: &mut impl FnMut(TmdsEvent),
    ) -> BringUpResult<S, P> {
        procedure::bring_up(&mut AsyncIo(self), link, video, config, record).await
    }
}

/// [`TmdsIo`] over a [`TmdsLink`]'s parts: each operation `.await`s the part's call.
struct AsyncIo<'a, S, P, K, D, X>(&'a mut TmdsLink<S, P, K, D, X>);

impl<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay, X: Cancel> TmdsIo for AsyncIo<'_, S, P, K, D, X> {
    type ScdcError = S::Error;
    type PhyError = P::Error;

    async fn write_tmds_config(&mut self, config: SinkTmdsConfig) -> Result<(), S::Error> {
        self.0.scdc.write_tmds_config(config).await
    }

    async fn update_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> Result<(), UpdateError<S::Error>> {
        self.0.scdc.update_tmds_config(config).await
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, S::Error> {
        self.0.scdc.read_tmds_config().await
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, S::Error> {
        self.0.scdc.read_scrambling_status().await
    }

    async fn read_sink_version(&mut self) -> Result<u8, S::Error> {
        self.0.scdc.read_sink_version().await
    }

    async fn write_source_version(&mut self, version: u8) -> Result<(), S::Error> {
        self.0.scdc.write_source_version(version).await
    }

    async fn clear_frl_rate(&mut self) -> Result<(), S::Error> {
        self.0.scdc.clear_frl_rate().await
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<S::Error>> {
        self.0.scdc.clear_flt_update().await
    }

    async fn configure(&mut self, config: TmdsPhyConfig) -> Result<(), P::Error> {
        self.0.phy.configure(config).await
    }

    async fn set_scrambling(&mut self, enabled: bool) -> Result<(), P::Error> {
        self.0.phy.set_scrambling(enabled).await
    }

    async fn start_output(&mut self) -> Result<(), P::Error> {
        self.0.phy.start_output().await
    }

    async fn stop_output(&mut self) -> Result<(), P::Error> {
        self.0.phy.stop_output().await
    }

    fn now(&self) -> Timestamp {
        self.0.clock.now()
    }

    async fn delay(&mut self, duration: Duration) {
        self.0.delay.delay(duration).await;
    }

    fn cancelled(&self) -> bool {
        self.0.cancel.is_cancelled()
    }
}
