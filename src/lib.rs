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
