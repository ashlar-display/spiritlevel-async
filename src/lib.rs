//! Async TMDS link bring-up for HDMI sources.
//!
//! `spiritlevel-async` is the async companion to `spiritlevel`. It has no procedures of its
//! own: [`TmdsLink`] runs spiritlevel's — bring-up, tear-down, the reconnect check and the
//! quick restore — over an async [`TmdsScdc`], hdmi-hal-async's [`TmdsPhy`] and [`Delay`],
//! a [`Clock`] and an optional [`Cancel`] flag, `.await`ing each operation. Sync and async
//! bring-up are one procedure, so they take the same steps and record the same
//! [`TmdsEvent`]s given the same I/O.
//!
//! The settings, outcomes and events are spiritlevel's, re-exported here with the
//! hdmi-hal-async types the API uses, so a caller or an implementation has one import
//! path.
//!
//! # Stopping a procedure
//!
//! A running procedure is stopped through its cancel flag ([`TmdsLink::with_cancel`]),
//! never by dropping its future: a dropped future stops at whichever `.await` it was
//! suspended on and cannot run its tear-down. Each procedure's documentation says what to
//! do after one.
//!
//! # `Send` futures
//!
//! The plain traits promise nothing about their futures. Each has a `Send` twin —
//! [`SendTmdsScdc`], [`SendTmdsPhy`], [`SendDelay`] — and in code bounded by the twins
//! alone, with a clock and a flag that are `Send` (and, for the `_with_events` forms, a
//! `Send` callback), every procedure's future is `Send`, so it
//! can be spawned on a multi-threaded executor.
//!
//! # Features
//!
//! - **`display-types`** (default) — spiritlevel's `From` conversions from display-types'
//!   `HdmiForumSinkCap` into the sink facts.
//! - **`culvert-async`** — [`TmdsScdc`] for culvert-async's client `Scdc<T>` and a borrow
//!   of it, with errors in spiritlevel's `CulvertError`.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

// Compiles and runs the README's examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(feature = "culvert-async")]
mod culvert;
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
