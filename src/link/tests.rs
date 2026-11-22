//! Tests of the async driver.

use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;

use super::TmdsLink;
use crate::{
    Cancel, Clock, DdcErrorKind, Delay, NoCancel, ScramblingStatus, SinkTmdsConfig, Timestamp,
    TmdsConfigReading, TmdsPhy, TmdsPhyCapabilities, TmdsPhyConfig, TmdsScdc, UpdateError,
};

/// A part of the driver, told apart by its number. It implements every part trait, but the
/// tests here run no procedure, so none of its I/O is ever called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Part(u8);

impl TmdsScdc for Part {
    type Error = DdcErrorKind;

    async fn write_tmds_config(&mut self, _: SinkTmdsConfig) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn update_tmds_config(
        &mut self,
        _: SinkTmdsConfig,
    ) -> Result<(), UpdateError<DdcErrorKind>> {
        unreachable!("no procedure runs")
    }

    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn read_sink_version(&mut self) -> Result<u8, DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn write_source_version(&mut self, _: u8) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn clear_frl_rate(&mut self) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<DdcErrorKind>> {
        unreachable!("no procedure runs")
    }
}

impl TmdsPhy for Part {
    type Error = DdcErrorKind;

    fn capabilities(&self) -> TmdsPhyCapabilities {
        TmdsPhyCapabilities::new(u32::from(self.0))
    }

    async fn configure(&mut self, _: TmdsPhyConfig) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn set_scrambling(&mut self, _: bool) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn start_output(&mut self) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }

    async fn stop_output(&mut self) -> Result<(), DdcErrorKind> {
        unreachable!("no procedure runs")
    }
}

impl Clock for Part {
    fn now(&self) -> Timestamp {
        Timestamp::from_micros(u64::from(self.0))
    }
}

impl Delay for Part {
    async fn delay(&mut self, _: Duration) {
        unreachable!("no procedure runs")
    }
}

#[test]
fn the_parts_come_back_as_given() {
    let link = TmdsLink::new(Part(1), Part(2), Part(3), Part(4));
    assert_eq!(link.scdc(), &Part(1));
    assert_eq!(link.phy(), &Part(2));
    assert_eq!(link.clock(), &Part(3));
    assert_eq!(link.clock().now(), Timestamp::from_micros(3));
    let (scdc, phy, clock, delay, cancel) = link.into_parts();
    assert_eq!(
        (scdc, phy, clock, delay),
        (Part(1), Part(2), Part(3), Part(4))
    );
    assert!(!cancel.is_cancelled());
}

#[test]
fn the_mutable_accessors_reach_the_parts_the_driver_holds() {
    let mut link = TmdsLink::new(Part(1), Part(2), Part(3), Part(4));
    *link.scdc_mut() = Part(10);
    link.phy_mut().0 = 20;
    assert_eq!(link.phy().capabilities().max_character_rate_khz, 20);
    let (scdc, phy, clock, delay, _) = link.into_parts();
    assert_eq!(
        (scdc, phy, clock, delay),
        (Part(10), Part(20), Part(3), Part(4))
    );
}

#[test]
fn with_cancel_replaces_only_the_flag() {
    let flag = AtomicBool::new(false);
    let link = TmdsLink::new(Part(1), Part(2), Part(3), Part(4)).with_cancel(&flag);
    assert!(!link.into_parts().4.is_cancelled());

    let link = TmdsLink::new(Part(1), Part(2), Part(3), Part(4)).with_cancel(&flag);
    flag.store(true, Ordering::Relaxed);
    let (scdc, phy, clock, delay, cancel) = link.into_parts();
    assert_eq!(
        (scdc, phy, clock, delay),
        (Part(1), Part(2), Part(3), Part(4))
    );
    assert!(core::ptr::eq(cancel, &flag), "the flag given, not a copy");
    assert!(cancel.is_cancelled(), "the driver sees the caller's store");

    // A second `with_cancel` replaces the first flag.
    let link = TmdsLink::new(Part(1), Part(2), Part(3), Part(4))
        .with_cancel(&flag)
        .with_cancel(NoCancel);
    assert!(!link.into_parts().4.is_cancelled());
}
