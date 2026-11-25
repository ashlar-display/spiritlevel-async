//! The sink-side operations the procedures make, async: [`TmdsScdc`] and its `Send` twin
//! [`SendTmdsScdc`].
//!
//! The counterpart of [`spiritlevel::TmdsScdc`]: the same eight operations, with the same
//! meaning and the same obligations on an implementation, each an `async fn`. With the
//! `culvert-async` feature, it is implemented for culvert-async's client.

use crate::{DdcError, ScramblingStatus, SinkTmdsConfig, TmdsConfigReading, UpdateError};

/// The sink's SCDC registers, as the TMDS procedures use them, async.
///
/// Each method has the contract of its namesake in [`spiritlevel::TmdsScdc`], whose
/// documentation says which procedure step makes each call, which register each touches and
/// what each returns; here the methods are `async`.
///
/// # Errors
///
/// [`Error`](Self::Error) is a [`DdcError`]: its kind tells the procedures which failures to
/// poll through and which to stop on, as with the sync trait. An operation made of a read and
/// a write returns [`UpdateError`], saying which of the two failed.
///
/// # Bounded calls
///
/// Each call completes in bounded time. The procedures read the cancel flag between calls,
/// not during one, so a call that never returns (a transfer the bus never finishes) cannot be
/// cancelled. An implementation bounds its calls and reports a transfer that does not finish
/// as an error of kind [`DdcErrorKind::Timeout`](crate::DdcErrorKind::Timeout).
///
/// # Cancellation
///
/// If a call's future is dropped before it completes, whether a write reached the sink is
/// unknown, and a caller that needs to know reads the register back. The procedures never
/// drop one: they are stopped through the driver's cancel flag. The clause matters only to
/// other code that calls the trait.
///
/// # `Send`
///
/// The futures are not required to be `Send`, so implementations for single-threaded
/// executors may hold `!Send` state across an `.await`. Code that spawns a procedure on a
/// multi-threaded executor bounds its SCDC implementation by [`SendTmdsScdc`] instead.
#[allow(
    async_fn_in_trait,
    reason = "the plain trait serves single-threaded executors; Send futures are the twin's, SendTmdsScdc"
)]
pub trait TmdsScdc {
    /// The error of one transfer, classified by [`DdcError::kind`].
    type Error: DdcError;

    /// See [`spiritlevel::TmdsScdc::write_tmds_config`].
    async fn write_tmds_config(&mut self, config: SinkTmdsConfig) -> Result<(), Self::Error>;

    /// See [`spiritlevel::TmdsScdc::update_tmds_config`].
    ///
    /// # Errors
    ///
    /// [`UpdateError::Read`] when the read failed (nothing written), [`UpdateError::Write`]
    /// when the write failed.
    async fn update_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> Result<(), UpdateError<Self::Error>>;

    /// See [`spiritlevel::TmdsScdc::read_tmds_config`].
    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, Self::Error>;

    /// See [`spiritlevel::TmdsScdc::read_scrambling_status`].
    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, Self::Error>;

    /// See [`spiritlevel::TmdsScdc::read_sink_version`].
    async fn read_sink_version(&mut self) -> Result<u8, Self::Error>;

    /// See [`spiritlevel::TmdsScdc::write_source_version`].
    async fn write_source_version(&mut self, version: u8) -> Result<(), Self::Error>;

    /// See [`spiritlevel::TmdsScdc::clear_frl_rate`].
    async fn clear_frl_rate(&mut self) -> Result<(), Self::Error>;

    /// See [`spiritlevel::TmdsScdc::clear_flt_update`].
    ///
    /// # Errors
    ///
    /// [`UpdateError::Read`] when the read failed (nothing known, nothing written),
    /// [`UpdateError::Write`] when the flag was set and the clear failed.
    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<Self::Error>>;
}

/// [`TmdsScdc`] with `Send` futures, for code that spawns TMDS bring-up on a multi-threaded
/// executor.
///
/// The same methods and contract, bounded calls and cancellation included, each returning a
/// future that is `Send`, and errors that are `Send` too; `Send` is a supertrait, since the
/// futures borrow the implementation mutably. Every `SendTmdsScdc` is a `TmdsScdc`, through a
/// blanket implementation, so the driver takes either; a type implements one of the two.
///
/// Bound generic code by `SendTmdsScdc` alone: with `S: SendTmdsScdc + TmdsScdc` the
/// compiler uses the explicit `TmdsScdc` bound instead of the blanket implementation, and the
/// futures are no longer known to be `Send`. With both traits in scope a method call on a
/// `SendTmdsScdc` type is ambiguous: import only the trait the code is bounded by, or call
/// `SendTmdsScdc::read_tmds_config(…)`.
pub trait SendTmdsScdc: Send {
    /// The error of one transfer, classified by [`DdcError::kind`].
    type Error: DdcError + Send;

    /// See [`TmdsScdc::write_tmds_config`].
    fn write_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// See [`TmdsScdc::update_tmds_config`].
    fn update_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> impl Future<Output = Result<(), UpdateError<Self::Error>>> + Send;

    /// See [`TmdsScdc::read_tmds_config`].
    fn read_tmds_config(
        &mut self,
    ) -> impl Future<Output = Result<TmdsConfigReading, Self::Error>> + Send;

    /// See [`TmdsScdc::read_scrambling_status`].
    fn read_scrambling_status(
        &mut self,
    ) -> impl Future<Output = Result<ScramblingStatus, Self::Error>> + Send;

    /// See [`TmdsScdc::read_sink_version`].
    fn read_sink_version(&mut self) -> impl Future<Output = Result<u8, Self::Error>> + Send;

    /// See [`TmdsScdc::write_source_version`].
    fn write_source_version(
        &mut self,
        version: u8,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// See [`TmdsScdc::clear_frl_rate`].
    fn clear_frl_rate(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// See [`TmdsScdc::clear_flt_update`].
    fn clear_flt_update(
        &mut self,
    ) -> impl Future<Output = Result<bool, UpdateError<Self::Error>>> + Send;
}

impl<T: SendTmdsScdc> TmdsScdc for T {
    type Error = <T as SendTmdsScdc>::Error;

    fn write_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        <T as SendTmdsScdc>::write_tmds_config(self, config)
    }

    fn update_tmds_config(
        &mut self,
        config: SinkTmdsConfig,
    ) -> impl Future<Output = Result<(), UpdateError<Self::Error>>> {
        <T as SendTmdsScdc>::update_tmds_config(self, config)
    }

    fn read_tmds_config(&mut self) -> impl Future<Output = Result<TmdsConfigReading, Self::Error>> {
        <T as SendTmdsScdc>::read_tmds_config(self)
    }

    fn read_scrambling_status(
        &mut self,
    ) -> impl Future<Output = Result<ScramblingStatus, Self::Error>> {
        <T as SendTmdsScdc>::read_scrambling_status(self)
    }

    fn read_sink_version(&mut self) -> impl Future<Output = Result<u8, Self::Error>> {
        <T as SendTmdsScdc>::read_sink_version(self)
    }

    fn write_source_version(
        &mut self,
        version: u8,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        <T as SendTmdsScdc>::write_source_version(self, version)
    }

    fn clear_frl_rate(&mut self) -> impl Future<Output = Result<(), Self::Error>> {
        <T as SendTmdsScdc>::clear_frl_rate(self)
    }

    fn clear_flt_update(&mut self) -> impl Future<Output = Result<bool, UpdateError<Self::Error>>> {
        <T as SendTmdsScdc>::clear_flt_update(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DdcErrorKind;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    /// Polls a future that completes without waiting, as the doubles' futures do.
    fn ready<F: Future>(future: F) -> F::Output {
        let mut cx = Context::from_waker(Waker::noop());
        match pin!(future).poll(&mut cx) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("the double's future waited"),
        }
    }

    /// A call that reached the twin, with its argument.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Call {
        WriteTmdsConfig(SinkTmdsConfig),
        UpdateTmdsConfig(SinkTmdsConfig),
        ReadTmdsConfig,
        ReadScramblingStatus,
        ReadSinkVersion,
        WriteSourceVersion(u8),
        ClearFrlRate,
        ClearFltUpdate,
    }

    /// A `SendTmdsScdc` that records each call and answers with fixed values.
    struct Recorder {
        calls: [Option<Call>; 8],
        count: usize,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                calls: [None; 8],
                count: 0,
            }
        }

        fn record(&mut self, call: Call) {
            self.calls[self.count] = Some(call);
            self.count += 1;
        }
    }

    impl SendTmdsScdc for Recorder {
        type Error = DdcErrorKind;

        async fn write_tmds_config(&mut self, config: SinkTmdsConfig) -> Result<(), Self::Error> {
            self.record(Call::WriteTmdsConfig(config));
            Ok(())
        }

        async fn update_tmds_config(
            &mut self,
            config: SinkTmdsConfig,
        ) -> Result<(), UpdateError<Self::Error>> {
            self.record(Call::UpdateTmdsConfig(config));
            Err(UpdateError::Write(DdcErrorKind::Timeout))
        }

        async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, Self::Error> {
            self.record(Call::ReadTmdsConfig);
            Ok(TmdsConfigReading::new(
                SinkTmdsConfig::new(true, false),
                0xF1,
            ))
        }

        async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, Self::Error> {
            self.record(Call::ReadScramblingStatus);
            Ok(ScramblingStatus::new(true, 0x81))
        }

        async fn read_sink_version(&mut self) -> Result<u8, Self::Error> {
            self.record(Call::ReadSinkVersion);
            Ok(1)
        }

        async fn write_source_version(&mut self, version: u8) -> Result<(), Self::Error> {
            self.record(Call::WriteSourceVersion(version));
            Err(DdcErrorKind::Detached)
        }

        async fn clear_frl_rate(&mut self) -> Result<(), Self::Error> {
            self.record(Call::ClearFrlRate);
            Ok(())
        }

        async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<Self::Error>> {
            self.record(Call::ClearFltUpdate);
            Ok(true)
        }
    }

    /// Every method through the plain trait alone, as the driver calls them.
    fn exercise<S: TmdsScdc<Error = DdcErrorKind>>(scdc: &mut S) {
        assert_eq!(
            ready(scdc.write_tmds_config(SinkTmdsConfig::new(true, true))),
            Ok(())
        );
        assert_eq!(
            ready(scdc.update_tmds_config(SinkTmdsConfig::new(false, true))),
            Err(UpdateError::Write(DdcErrorKind::Timeout))
        );
        let reading = ready(scdc.read_tmds_config()).unwrap();
        assert_eq!(
            (reading.config, reading.raw),
            (SinkTmdsConfig::new(true, false), 0xF1)
        );
        let status = ready(scdc.read_scrambling_status()).unwrap();
        assert_eq!((status.set, status.raw()), (true, 0x81));
        assert_eq!(ready(scdc.read_sink_version()), Ok(1));
        assert_eq!(
            ready(scdc.write_source_version(1)),
            Err(DdcErrorKind::Detached)
        );
        assert_eq!(ready(scdc.clear_frl_rate()), Ok(()));
        assert_eq!(ready(scdc.clear_flt_update()), Ok(true));
    }

    #[test]
    fn every_call_through_the_plain_trait_reaches_the_twin() {
        let mut scdc = Recorder::new();
        exercise(&mut scdc);
        assert_eq!(
            scdc.calls,
            [
                Some(Call::WriteTmdsConfig(SinkTmdsConfig::new(true, true))),
                Some(Call::UpdateTmdsConfig(SinkTmdsConfig::new(false, true))),
                Some(Call::ReadTmdsConfig),
                Some(Call::ReadScramblingStatus),
                Some(Call::ReadSinkVersion),
                Some(Call::WriteSourceVersion(1)),
                Some(Call::ClearFrlRate),
                Some(Call::ClearFltUpdate),
            ]
        );
    }
}
