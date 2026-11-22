//! Async TMDS link bring-up for HDMI sources.
//!
//! `spiritlevel-async` is the async companion to `spiritlevel`: it runs spiritlevel's
//! procedures (bring-up, tear-down, the reconnect check and the quick restore) over
//! hdmi-hal-async's PHY, clock and delay traits and an async SCDC trait of its own, so sync
//! and async bring-up are the same procedure.
//!
//! # Features
//!
//! - **`display-types`** (default) — spiritlevel's conversion of display-types'
//!   HF-SCDB type into the sink facts.
//! - **`culvert-async`** — the SCDC trait implemented for culvert-async's client and a
//!   borrow of it, and its `Send` twin for the `SendScdc` wrapper.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod link;
mod scdc;

pub use link::TmdsLink;
pub use scdc::{SendTmdsScdc, TmdsScdc};

// spiritlevel's link parameters, SCDC values, settings, outcomes, errors and events,
// re-exported so that callers name the same versions this crate's procedures use. The
// sync `TmdsScdc` and `TmdsLink` are not re-exported: this crate's own carry those names.

// The link parameters.
pub use spiritlevel::{
    ClockRatio, ParamsError, ScramblingDecision, ScramblingOff, ScramblingThreshold, SinkTmdsFacts,
    SourceTmdsFacts, TmdsConfigGate, TmdsLinkParams, TmdsPolicy,
};
// The SCDC values.
pub use spiritlevel::{ScramblingStatus, SinkTmdsConfig, TmdsConfigReading, UpdateError};
// The settings.
pub use spiritlevel::{
    BringUpConfig, ClearFrl, ConfigError, OnTmdsConfigFailure, OutputDuringSinkWrite, ScramblerOff,
    SinkClearTiming, SinkPresence, SourceVersionWrite, TearDownConfig, TmdsConfigWrite,
};
// The outcomes and errors.
pub use spiritlevel::{
    BringUpError, BringUpOutcome, BringUpWarning, CheckResult, Cleanup, FailedRead, LinkUp,
    OutputState, ReconnectCheck, RestoreError, Restored, ScramblingCheck, SinkClear, TearDownError,
    TornDown, Warnings,
};
// The events.
pub use spiritlevel::{
    CancelPoint, FrlClearSkip, MAX_BRING_UP_EVENTS, MAX_QUICK_RESTORE_EVENTS,
    MAX_RECONNECT_CHECK_EVENTS, MAX_TEAR_DOWN_EVENTS, SinkClearSkip, SourceVersionSkip,
    TmdsConfigSkip, TmdsEvent, TmdsEventKind, TmdsStep,
};

// The sink capabilities the sink facts convert from, re-exported from display-types
// through spiritlevel.
#[cfg(feature = "display-types")]
pub use spiritlevel::HdmiForumSinkCap;

// The hdmi-hal-async traits and types the driver's signatures use, re-exported so that
// callers and implementations name the same versions this crate does: the cancel flag,
// the SCDC error classification, the PHY and its `Send` twin, the clock, the delay and its
// `Send` twin, and the colour depth and format a `TmdsVideo` carries.
pub use hdmi_hal_async::cancel::{Cancel, NoCancel};
pub use hdmi_hal_async::ddc::{DdcError, DdcErrorKind, NoAcknowledgeSource};
pub use hdmi_hal_async::phy::{
    SendTmdsPhy, TmdsPhy, TmdsPhyCapabilities, TmdsPhyConfig, TmdsVideo,
};
pub use hdmi_hal_async::time::{Clock, Delay, SendDelay, Timestamp};
pub use hdmi_hal_async::{ColorBitDepth, ColorFormat};

// culvert-async's client, its errors and the transports it runs over, re-exported for the
// adapter's users.
#[cfg(feature = "culvert-async")]
pub use culvert_async::{ProtocolError, Scdc, ScdcError, ScdcTransport, SendScdcTransport};
// The error of culvert's client as the SCDC trait reports it, re-exported from spiritlevel
// so that the sync and async adapters return the same type.
#[cfg(feature = "culvert-async")]
pub use spiritlevel::CulvertError;
