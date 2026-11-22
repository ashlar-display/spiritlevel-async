//! The async driver: [`TmdsLink`] runs spiritlevel's procedures over an async [`TmdsScdc`],
//! an async `TmdsPhy`, a `Clock` and an async `Delay`, with an optional [`Cancel`] flag.

use crate::{Cancel, Clock, Delay, NoCancel, TmdsPhy, TmdsScdc};

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
}
