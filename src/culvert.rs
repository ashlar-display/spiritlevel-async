//! [`TmdsScdc`] for culvert-async's SCDC client (the `culvert-async` feature).
//!
//! Implemented for `culvert_async::Scdc<T>` itself and for a mutable borrow of it, over any
//! async `ScdcTransport`, and [`SendTmdsScdc`] for the [`SendScdc`] wrapper over a
//! `SendScdcTransport`. Errors are culvert's, wrapped in spiritlevel's [`CulvertError`] so
//! that they carry a [`DdcErrorKind`](crate::DdcErrorKind), the same type spiritlevel's
//! sync adapter returns.

use culvert_async::{
    ClearableUpdateFlags, FfeLevels, FrlConfig, FrlRate, ReadModifyWriteError, Scdc, ScdcTransport,
    SendScdcTransport, TmdsConfig,
};

use crate::{
    CulvertError, ScramblingStatus, SendTmdsScdc, SinkTmdsConfig, TmdsConfigReading, TmdsScdc,
    UpdateError,
};

fn config(config: SinkTmdsConfig) -> TmdsConfig {
    TmdsConfig {
        scrambling_enable: config.scrambling_enable,
        high_tmds_clock_ratio: config.high_tmds_clock_ratio,
    }
}

fn update_error<E>(error: ReadModifyWriteError<E>) -> UpdateError<CulvertError<E>> {
    match error {
        ReadModifyWriteError::Read(e) => UpdateError::Read(CulvertError(e)),
        ReadModifyWriteError::Write(e) => UpdateError::Write(CulvertError(e)),
    }
}

/// culvert-async's client, as spiritlevel's culvert adapter implements the sync trait for
/// culvert's: `update_tmds_config` is the client's read-modify-write, and the bytes read
/// reach [`TmdsConfigReading::raw`] and [`ScramblingStatus::raw`].
///
/// The plain trait, not the twin: implementing both for one type would conflict with the
/// blanket implementation. Its futures are `Send` all the same where the compiler can see
/// the transport: in code generic over `T: SendScdcTransport` that names `Scdc<T>`, the
/// driver's futures are `Send` under the conditions [`TmdsLink`](crate::TmdsLink) gives.
impl<T: ScdcTransport> TmdsScdc for Scdc<T> {
    type Error = CulvertError<T::Error>;

    async fn write_tmds_config(&mut self, sink: SinkTmdsConfig) -> Result<(), Self::Error> {
        Ok(Scdc::write_tmds_config(self, config(sink)).await?)
    }

    async fn update_tmds_config(
        &mut self,
        sink: SinkTmdsConfig,
    ) -> Result<(), UpdateError<Self::Error>> {
        Scdc::update_tmds_config(self, config(sink))
            .await
            .map_err(update_error)
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, Self::Error> {
        let reading = Scdc::read_tmds_config(self).await?;
        Ok(TmdsConfigReading::new(
            SinkTmdsConfig::new(
                reading.config.scrambling_enable,
                reading.config.high_tmds_clock_ratio,
            ),
            reading.raw,
        ))
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, Self::Error> {
        let status = self.read_scrambler_status().await?;
        Ok(ScramblingStatus::new(status.scrambling_active, status.raw))
    }

    async fn read_sink_version(&mut self) -> Result<u8, Self::Error> {
        Ok(Scdc::read_sink_version(self).await?)
    }

    async fn write_source_version(&mut self, version: u8) -> Result<(), Self::Error> {
        Ok(Scdc::write_source_version(self, version).await?)
    }

    async fn clear_frl_rate(&mut self) -> Result<(), Self::Error> {
        // `Config_1` as 0: no FRL rate, FFE levels 0.
        Ok(self
            .write_frl_config(FrlConfig::new(FrlRate::NotSupported, FfeLevels::default()))
            .await?)
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<Self::Error>> {
        let flags = self
            .read_update_flags()
            .await
            .map_err(|e| UpdateError::Read(CulvertError(e)))?;
        if flags.flt_update {
            let only_flt_update =
                ClearableUpdateFlags::new(false, false, false, false, true, false);
            self.clear_update_flags(only_flt_update)
                .await
                .map_err(|e| UpdateError::Write(CulvertError(e)))?;
        }
        Ok(flags.flt_update)
    }
}

/// The client lent to the driver, so one client serves the SCDC setup, FRL training, TMDS
/// bring-up and health reads in turn without being moved. Its futures are `Send` under the
/// same conditions as the client's.
impl<T: ScdcTransport> TmdsScdc for &mut Scdc<T> {
    type Error = CulvertError<T::Error>;

    async fn write_tmds_config(&mut self, sink: SinkTmdsConfig) -> Result<(), Self::Error> {
        TmdsScdc::write_tmds_config(&mut **self, sink).await
    }

    async fn update_tmds_config(
        &mut self,
        sink: SinkTmdsConfig,
    ) -> Result<(), UpdateError<Self::Error>> {
        TmdsScdc::update_tmds_config(&mut **self, sink).await
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, Self::Error> {
        TmdsScdc::read_tmds_config(&mut **self).await
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, Self::Error> {
        TmdsScdc::read_scrambling_status(&mut **self).await
    }

    async fn read_sink_version(&mut self) -> Result<u8, Self::Error> {
        TmdsScdc::read_sink_version(&mut **self).await
    }

    async fn write_source_version(&mut self, version: u8) -> Result<(), Self::Error> {
        TmdsScdc::write_source_version(&mut **self, version).await
    }

    async fn clear_frl_rate(&mut self) -> Result<(), Self::Error> {
        TmdsScdc::clear_frl_rate(&mut **self).await
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<Self::Error>> {
        TmdsScdc::clear_flt_update(&mut **self).await
    }
}

/// culvert-async's client as a [`SendTmdsScdc`], for code generic over `S: SendTmdsScdc`.
///
/// Such code accepts no plain [`TmdsScdc`] such as `Scdc<T>`: an orchestrator generic over
/// its SCDC implementation, for example. Code that names `Scdc<T>` itself needs no wrapper:
/// its futures are `Send` over a `SendScdcTransport` already. The field is public, so the
/// client is reached and taken back directly.
pub struct SendScdc<T>(pub Scdc<T>);

impl<T: SendScdcTransport> SendTmdsScdc for SendScdc<T> {
    type Error = CulvertError<T::Error>;

    async fn write_tmds_config(&mut self, sink: SinkTmdsConfig) -> Result<(), Self::Error> {
        TmdsScdc::write_tmds_config(&mut self.0, sink).await
    }

    async fn update_tmds_config(
        &mut self,
        sink: SinkTmdsConfig,
    ) -> Result<(), UpdateError<Self::Error>> {
        TmdsScdc::update_tmds_config(&mut self.0, sink).await
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

#[cfg(test)]
mod tests {
    use core::future::Future;
    use core::pin::pin;
    use core::sync::atomic::{AtomicU32, Ordering};
    use core::task::{Context, Poll, Waker};

    use culvert_async::ScdcError;

    use super::*;
    use crate::{DdcError, DdcErrorKind, NoAcknowledgeSource};

    /// Polls a future whose I/O completes at once.
    fn ready<F: Future>(future: F) -> F::Output {
        match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(output) => output,
            Poll::Pending => unreachable!("the register file never waits"),
        }
    }

    const NACK: DdcErrorKind = DdcErrorKind::NoAcknowledge(NoAcknowledgeSource::Address);

    /// An async transport over a register file, failing every transfer from `fail_from` on
    /// with a NACK. `Update_0` is write-1-to-clear, as on a sink. The twin, so it serves the
    /// plain implementations (through hdmi-hal-async's blanket implementation) and
    /// `SendScdc` alike.
    struct Sink {
        regs: [u8; 0x40],
        transfers: AtomicU32,
        fail_from: u32,
    }

    impl Sink {
        fn new() -> Self {
            Self {
                regs: [0; 0x40],
                transfers: AtomicU32::new(0),
                fail_from: u32::MAX,
            }
        }

        fn transfer(&self) -> Result<(), DdcErrorKind> {
            if self.transfers.fetch_add(1, Ordering::Relaxed) >= self.fail_from {
                Err(NACK)
            } else {
                Ok(())
            }
        }
    }

    impl SendScdcTransport for Sink {
        type Error = DdcErrorKind;

        async fn read(&self, reg: u8) -> Result<u8, DdcErrorKind> {
            self.transfer().map(|()| self.regs[usize::from(reg)])
        }

        async fn write(&mut self, reg: u8, value: u8) -> Result<(), DdcErrorKind> {
            self.transfer()?;
            if reg == 0x10 {
                self.regs[0x10] &= !value;
            } else {
                self.regs[usize::from(reg)] = value;
            }
            Ok(())
        }

        async fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), DdcErrorKind> {
            self.transfer()?;
            let start = usize::from(reg);
            buf.copy_from_slice(&self.regs[start..start + buf.len()]);
            Ok(())
        }
    }

    /// A sink with each register the trait touches set to a value that tells it apart.
    fn sink() -> Sink {
        let mut sink = Sink::new();
        sink.regs[0x01] = 1;
        sink.regs[0x10] = 0x23;
        sink.regs[0x20] = 0xF0;
        sink.regs[0x21] = 0x81;
        sink.regs[0x31] = 0x53;
        sink
    }

    /// Every method through `scdc`, with what each returns.
    fn exercise<S: TmdsScdc<Error = CulvertError<DdcErrorKind>>>(scdc: &mut S) {
        assert_eq!(ready(scdc.read_sink_version()).unwrap(), 1);
        ready(scdc.write_source_version(1)).unwrap();
        ready(scdc.clear_frl_rate()).unwrap();
        assert!(
            ready(scdc.clear_flt_update()).unwrap(),
            "FLT_update was set"
        );
        assert!(!ready(scdc.clear_flt_update()).unwrap(), "and is now clear");

        ready(scdc.update_tmds_config(SinkTmdsConfig::new(true, true))).unwrap();
        let read = ready(scdc.read_tmds_config()).unwrap();
        assert_eq!(read.config, SinkTmdsConfig::new(true, true));
        assert_eq!(read.raw, 0xF3, "the other bits kept, the byte reported");
        let status = ready(scdc.read_scrambling_status()).unwrap();
        assert!(status.set);
        assert_eq!(status.raw(), 0x81);
        ready(scdc.write_tmds_config(SinkTmdsConfig::new(false, false))).unwrap();
    }

    /// The registers `exercise` leaves behind.
    fn assert_exercised(sink: &Sink) {
        assert_eq!(sink.regs[0x02], 1, "Source_Version");
        assert_eq!(sink.regs[0x31], 0, "Config_1 cleared");
        assert_eq!(sink.regs[0x10], 0x03, "only FLT_update cleared");
        assert_eq!(sink.regs[0x20], 0x00, "TMDS_Config written whole");
    }

    #[test]
    fn every_method_goes_through_the_client() {
        let mut scdc = Scdc::new(sink());
        exercise(&mut scdc);
        assert_exercised(&scdc.into_transport());
    }

    #[test]
    fn a_borrowed_client_forwards_every_call() {
        let mut scdc = Scdc::new(sink());
        let mut borrowed = &mut scdc;
        exercise(&mut borrowed);
        assert_exercised(&scdc.into_transport());
    }

    #[test]
    fn the_send_wrapper_forwards_every_call_to_the_client() {
        let mut scdc = SendScdc(Scdc::new(sink()));
        // Through the plain trait, which the twin's blanket implementation gives it.
        exercise(&mut scdc);
        assert_exercised(&scdc.0.into_transport());
    }

    #[test]
    fn errors_carry_the_transport_kind_and_say_which_transfer_failed() {
        let mut failing = Sink::new();
        failing.fail_from = 0;
        let mut scdc = Scdc::new(failing);
        let error = ready(TmdsScdc::read_sink_version(&mut scdc)).unwrap_err();
        assert_eq!(error, CulvertError(ScdcError::Transport(NACK)));
        assert_eq!(error.kind(), NACK);
        assert!(matches!(
            ready(TmdsScdc::update_tmds_config(&mut scdc, SinkTmdsConfig::new(true, true))),
            Err(UpdateError::Read(e)) if e.kind() == NACK
        ));
        assert!(matches!(
            ready(TmdsScdc::clear_flt_update(&mut scdc)),
            Err(UpdateError::Read(e)) if e.kind() == NACK
        ));

        // The read passes, the write fails.
        let mut after_read = Sink::new();
        after_read.regs[0x10] = 0x20;
        after_read.fail_from = 1;
        let mut scdc = Scdc::new(after_read);
        assert!(matches!(
            ready(TmdsScdc::clear_flt_update(&mut scdc)),
            Err(UpdateError::Write(e)) if e.kind() == NACK
        ));
        let mut after_read = Sink::new();
        after_read.regs[0x20] = 0x80;
        after_read.fail_from = 1;
        let mut scdc = Scdc::new(after_read);
        assert!(matches!(
            ready(TmdsScdc::update_tmds_config(&mut scdc, SinkTmdsConfig::new(true, true))),
            Err(UpdateError::Write(e)) if e.kind() == NACK
        ));
        assert_eq!(scdc.into_transport().regs[0x20], 0x80, "nothing written");
    }

    #[test]
    fn the_transport_reads_blocks_as_a_sink_would() {
        let mut sink = Sink::new();
        sink.regs[0x3E] = 0xAB;
        sink.regs[0x3F] = 0xCD;
        let mut buf = [0; 2];
        ready(SendScdcTransport::read_block(&sink, 0x3E, &mut buf)).unwrap();
        assert_eq!(buf, [0xAB, 0xCD]);
        sink.fail_from = 0;
        assert!(ready(SendScdcTransport::read_block(&sink, 0, &mut buf)).is_err());
    }
}
