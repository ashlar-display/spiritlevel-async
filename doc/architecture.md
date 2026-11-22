# Architecture

This document describes `spiritlevel-async` as released: what it adds to
[spiritlevel](https://github.com/ashlar-display/spiritlevel), its traits and driver, when
its futures are `Send`, how a procedure is stopped, and why it works the way it does. What
the procedures do, their settings, outcomes and events, and the sources for their rules
are spiritlevel's, in its architecture document.

## Role

`spiritlevel-async` is the async companion to `spiritlevel`: TMDS link bring-up for async
SCDC, PHY and delay implementations. It has no procedures of its own. It runs spiritlevel's
— bring-up, tear-down, the reconnect check and the quick restore, written once in
`spiritlevel::procedure` as `async fn`s that perform no I/O themselves — over an async
`TmdsScdc` and hdmi-hal-async's `TmdsPhy`, `Clock` and `Delay`, `.await`ing each
operation, so a bring-up yields to the executor wherever the I/O or a wait does.

Because there is one implementation of each procedure, sync and async bring-up behave
identically, step for step and event for event, and every fix reaches both.

The crate suits cooperative async runtimes on embedded targets (embassy) and OS targets
(tokio); it depends on no runtime.

---

## Scope

`spiritlevel-async` covers:

- **`TmdsScdc`:** the async SCDC trait the procedures need, the counterpart of
  `spiritlevel::TmdsScdc`, and its `Send` twin `SendTmdsScdc`.
- **`TmdsLink<S, P, K, D, X>`:** owns an async `TmdsScdc`, an async `TmdsPhy`, a `Clock`, an
  async `Delay` and an optional `Cancel` flag, and runs spiritlevel's procedures over them,
  each in a plain and a `_with_events` form.
- **The culvert-async adapter** (`culvert-async` feature): `TmdsScdc` for
  [culvert-async](https://github.com/ashlar-display/culvert-async)'s SCDC client.
- **Re-exports** of spiritlevel's link parameters, settings, outcomes and events, and of the
  hdmi-hal-async traits and types the API uses, so a caller has one import path.

It does not cover:

- **The procedures themselves:** their steps, settings, outcomes, events and the rules
  behind them are spiritlevel's.
- **Choosing the link:** `TmdsLinkParams::choose` is spiritlevel's pure `const fn`,
  re-exported here; it needs no I/O and so no async form.
- **The hardware:** the PHY's registers and the SCDC transfers are the implementations of
  the traits.
- **Retries, hotplug and when to bring the link up or down:** the caller's, as with
  spiritlevel.
- **An executor:** nothing here spawns, selects or depends on a runtime.

---

## Dependencies and features

The features and the dependencies, with their versions in `Cargo.toml`:

```toml
[features]
default = ["display-types"]
display-types = ["spiritlevel/display-types"]
culvert-async = ["dep:culvert-async", "spiritlevel/culvert"]

[dependencies]
spiritlevel = { default-features = false }
hdmi-hal-async = { default-features = false }
culvert-async = { default-features = false, optional = true }
```

```
display-types ─► hdmi-hal ─┬─► hdmi-hal-async ──────────┬─► spiritlevel-async
                           ├─► spiritlevel ─────────────┤
                           └─► culvert ─► culvert-async ┘  (culvert-async feature)
```

- **spiritlevel** (always): the procedures (`spiritlevel::procedure`) and every type their
  signatures use, re-exported here.
- **hdmi-hal-async** (always): the async `TmdsPhy`, `Delay` and their `Send` twins, the
  `Clock`, the cancel flag and the DDC error classification. Both are public dependencies:
  a breaking release of either is a breaking release of this crate.
- **`display-types`** (default): spiritlevel's `From` conversions from display-types'
  `HdmiForumSinkCap` into the sink facts.
- **`culvert-async`**: the adapter ("The culvert-async adapter"). It turns on
  spiritlevel's `culvert` feature for `CulvertError`, so the sync and async adapters return
  the same error type; culvert-async depends on culvert itself, so this adds no crate to the
  graph.

---

## The async `TmdsScdc`

The counterpart of `spiritlevel::TmdsScdc`: the same eight operations with the same meaning
and the same obligations on an implementation, each an `async fn`.

```rust
pub trait TmdsScdc {
    type Error: DdcError;

    async fn write_tmds_config(&mut self, config: SinkTmdsConfig) -> Result<(), Self::Error>;
    async fn update_tmds_config(&mut self, config: SinkTmdsConfig) -> Result<(), UpdateError<Self::Error>>;
    async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, Self::Error>;
    async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, Self::Error>;
    async fn read_sink_version(&mut self) -> Result<u8, Self::Error>;
    async fn write_source_version(&mut self, version: u8) -> Result<(), Self::Error>;
    async fn clear_frl_rate(&mut self) -> Result<(), Self::Error>;
    async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<Self::Error>>;
}
```

The values are spiritlevel's (`SinkTmdsConfig`, `TmdsConfigReading`, `ScramblingStatus`,
`UpdateError`), and the error is a `DdcError`, whose kind tells the procedures which
failures to poll through and which to stop on.

**Cancellation clause,** as hdmi-hal-async's `ScdcTransport` has: if a call's future is
dropped, whether a write reached the sink is unknown, and a caller that needs to know reads
the register back. The procedures never drop one ("Cancellation" below), so the clause
matters only to other code that calls the trait.

MSRV is 1.85, which has `async fn` in traits (stable since 1.75); no `async-trait` crate is
needed. The futures are not required to be `Send`, and the trait is not object-safe: the
trade-offs hdmi-hal-async and embedded-hal-async make.

### `Send` futures: `SendTmdsScdc`

Code generic over the plain trait cannot spawn a future that calls it, however it bounds the
type: an `async fn` in a trait promises nothing about its future. `SendTmdsScdc` is the
`Send` twin, as `SendTmdsPhy` and `SendDelay` are hdmi-hal-async's: the same methods, each
returning a future that is `Send`, a `Send` error, and `Send` as a supertrait, since the
futures borrow the implementation mutably. A blanket implementation makes every
`SendTmdsScdc` a `TmdsScdc`, so `TmdsLink` takes either.

Generic code is bounded by the twins alone: with `S: SendTmdsScdc + TmdsScdc` the compiler
uses the explicit `TmdsScdc` bound instead of the blanket implementation, and the futures
are opaque again. The same holds for `SendTmdsPhy` with `+ TmdsPhy` and `SendDelay` with
`+ Delay`.

The blanket implementation costs this crate the forwarding implementations
spiritlevel's trait has (for `&mut T`): they would overlap with it (E0119). A caller that
needs its SCDC implementation back between procedures takes it from the driver
(`scdc_mut`, `into_parts`). Other crates lose nothing: they implement either trait for
their own types, and the orphan rule never let them write a blanket implementation of it.

---

## `TmdsLink<S, P, K, D, X>`

```rust
pub struct TmdsLink<S, P, K, D, X = NoCancel> { /* the five parts */ }

impl<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay> TmdsLink<S, P, K, D, NoCancel> {
    pub fn new(scdc: S, phy: P, clock: K, delay: D) -> Self;
}

impl<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay, X: Cancel> TmdsLink<S, P, K, D, X> {
    pub fn with_cancel<X2: Cancel>(self, cancel: X2) -> TmdsLink<S, P, K, D, X2>;
    pub fn into_parts(self) -> (S, P, K, D, X);
    // scdc, scdc_mut, phy, phy_mut, clock: the parts between procedures

    pub async fn bring_up(&mut self, link: &TmdsLinkParams, video: TmdsVideo, config: &BringUpConfig)
        -> Result<BringUpOutcome<S::Error, P::Error>, BringUpError<S::Error, P::Error>>;
    pub async fn bring_up_with_events(&mut self, link: &TmdsLinkParams, video: TmdsVideo,
        config: &BringUpConfig, record: &mut impl FnMut(TmdsEvent)) -> …;
    pub async fn tear_down(&mut self, link: &TmdsLinkParams, config: &TearDownConfig,
        presence: SinkPresence) -> Result<TornDown<S::Error>, TearDownError<S::Error, P::Error>>;
    pub async fn check_reconnect(&mut self, link: &TmdsLinkParams, current_sink: SinkTmdsFacts)
        -> ReconnectCheck<S::Error>;
    pub async fn quick_restore(&mut self, link: &TmdsLinkParams, config: &BringUpConfig)
        -> Result<Restored<S::Error>, RestoreError<S::Error, P::Error>>;
    // and the `_with_events` form of each
}
```

`TmdsPhy` and `Delay` are hdmi-hal-async's; `Clock` and `Cancel` are hdmi-hal's, which
hdmi-hal-async re-exports: reading the clock and the flag does not wait. The API is
`spiritlevel::TmdsLink`'s, with the procedures `async`.

Each procedure runs its `spiritlevel::procedure` function over a private adapter that
implements `spiritlevel::procedure::TmdsIo` by `.await`ing the driver's parts. The outcomes,
errors and events are therefore exactly spiritlevel's: a sync and an async bring-up against
the same sink record the same events. The procedures' bounds on events
(`MAX_BRING_UP_EVENTS` and the others) hold here too, so a caller without `alloc` sizes a
channel from them and never drops an event.

**One value as clock and delay**, as with spiritlevel: implement `Clock` and `Delay` for a
reference to it and pass the reference twice. For a `Send` future, the shared value must be
`Sync` (an atomic rather than a `Cell`), or the clock and the delay kept separate, as
embassy-time's and tokio's are.

### `Send` futures

The futures of every procedure are `Send` in generic code when the parts are bounded by the
twins alone — `S: SendTmdsScdc`, `P: SendTmdsPhy`, `D: SendDelay` — and the clock and the
flag are `Send` (`K: Clock + Send`, `X: Cancel + Send`, as `&AtomicBool` is); the
`_with_events` forms also need a `Send` callback. spiritlevel's procedures hold nothing
across an `.await` that is not `Send`, so the driver adds no condition of its own. A test
checks this at compile time, generic over the twins.

### Cancellation

A caller stops a running procedure through its `Cancel` flag, never by dropping its future.
The flag is read where spiritlevel says (at entry, before each SCDC step, before each slice
of a wait and before each read of the check), and a bring-up it stops ends
`BringUpOutcome::Cancelled`, after the tear-down it carries.

Dropping the future instead — racing it with `select`, wrapping it in a timeout, aborting
its task — stops the procedure at whichever `.await` it was suspended on: the sink may hold
a new `TMDS_Config`, the PHY may be configured or transmitting, a transfer may be cut short
(the parts' cancellation clauses say what each leaves), and no tear-down runs, since a
dropped future cannot await anything. The driver keeps no state between calls, so it stays
usable: after a dropped procedure, call `tear_down` before anything else. The procedures
bound their own timing (the ratio wait, the check's window, the deadline), so a timeout
around them adds nothing.

---

## The culvert-async adapter

With the `culvert-async` feature, `TmdsScdc` is implemented for culvert-async's
`Scdc<T>` itself, over any async `ScdcTransport`, as spiritlevel's `culvert` feature does
for culvert's client. Its error is spiritlevel's `CulvertError<E>`, wrapping culvert's
`ScdcError<E>`: its kind is the transport's own for a transport error and `Unsupported`
for a request culvert refuses. `update_tmds_config` is culvert-async's read-modify-write, and the
bytes read reach `TmdsConfigReading::raw` and `ScramblingStatus::raw`.

The adapter implements the plain trait, not the twin: `Scdc<T>` is one concrete type, and
implementing both for it would conflict with the blanket implementation. Its futures are
`Send` all the same where the compiler can see the transport: in code generic over
`T: SendScdcTransport` that names `Scdc<T>`, a `TmdsLink<Scdc<T>, P, K, D, X>` has `Send`
futures under the conditions above. The compile-time test covers this case as well.

---

## On embassy and tokio

**embassy:**

- **Clock and delay from `embassy-time`:** `now` is `Timestamp::from_micros(Instant::now()
  .as_micros())`, the delay `Timer::after_micros(…)` with the duration rounded up to whole
  microseconds. The default 1 MHz tick is well inside the millisecond the procedures need,
  and `Timer`'s futures are `Send`, so `SendDelay` holds.
- **Events into a channel:** the callback runs between `.await`s and cannot await, so it
  hands each event on with `embassy_sync::channel::Channel::try_send`, the channel sized
  from the procedure's maximum.
- **Cancel with the flag, not `select`:** the task that watches the hotplug line sets an
  `AtomicBool` the driver holds (`with_cancel(&FLAG)`), and the procedure ends `Cancelled`
  with its tear-down done.

**tokio:**

- **`Send` is required:** `tokio::spawn` on the multi-threaded runtime needs `Send` futures,
  so a tokio caller implements the twins and passes a `Send` callback
  (`tokio::sync::mpsc::Sender::try_send`, the channel sized from the procedure's maximum).
- **More ways to drop a future:** besides `select!`, `tokio::time::timeout` and aborting a
  `JoinHandle` drop the procedure mid-flight; set the cancel flag instead.
- **Timing is correct, not exact:** `tokio::time::sleep` rounds up to its millisecond
  granularity, so a 1 ms interval polls less often (a delay promises "at least"), and a
  loaded runtime may wake late, which the outcome reports (a later output start, at worst a
  missed-deadline warning). The clock is `std`'s monotonic `Instant`, measured from a start
  `Instant`.
- **On Linux the I/O blocks:** the stack's Linux backend, hdmi-hal-i2c-dev, is synchronous.
  Under tokio, run spiritlevel's sync `TmdsLink` on a thread of its own or in
  `spawn_blocking`, and send its events back over a channel; this crate is for async I/O.

---

## Re-exports

Every type a caller or an implementation names comes from the crate root, at the versions
this crate uses:

- **spiritlevel's:** the link parameters (`TmdsLinkParams`, `ClockRatio`,
  `ScramblingDecision`, `ScramblingOff`, `ParamsError`, the sink and source facts,
  `TmdsPolicy` and its threshold and gate), the SCDC values (`SinkTmdsConfig`, `TmdsConfigReading`,
  `ScramblingStatus`, `UpdateError`), the settings (`BringUpConfig`, `TearDownConfig`,
  `SinkPresence` and their options), the outcomes and errors (`BringUpOutcome`,
  `BringUpError`, `TornDown`, `ReconnectCheck`, `Restored` and the rest), and the events
  (`TmdsEvent`, `TmdsEventKind`, `TmdsStep`, the skip reasons, `CancelPoint` and the
  `MAX_*_EVENTS` bounds); `HdmiForumSinkCap` with `display-types`.
- **hdmi-hal-async's:** `TmdsPhy` and `SendTmdsPhy` with `TmdsPhyCapabilities`,
  `TmdsPhyConfig` and `TmdsVideo` and its colour types; `Clock`, `Delay`, `SendDelay` and
  `Timestamp`; `Cancel` and `NoCancel`; `DdcError`, `DdcErrorKind` and
  `NoAcknowledgeSource`.
- **With `culvert-async`:** `CulvertError`, and culvert-async's `Scdc`, `ScdcTransport`,
  `SendScdcTransport`, `ScdcError` and `ProtocolError`.

The sync `TmdsScdc` and `TmdsLink` are not re-exported: this crate's own carry those names.

---

## Module structure

```
src/
  lib.rs          — #![no_std], re-exports, module declarations, crate-level docs
  scdc.rs         — the async TmdsScdc and its Send twin SendTmdsScdc
  link/mod.rs     — TmdsLink and its TmdsIo adapter
  link/tests.rs   — tests of the async driver
  culvert.rs      — TmdsScdc for culvert-async's Scdc<T> (culvert-async feature)
```

---

## `no_std` and `alloc`

`#![no_std]` without `alloc` in every build, and `#![forbid(unsafe_code)]`: the procedures'
outcomes use fixed storage and their events a callback, as in spiritlevel. CI builds the
crate for a bare target (`thumbv7em-none-eabi`) with and without default features and with
`culvert-async`, and checks the MSRV (Rust 1.85).

---

## Design Principles

- **One implementation, sync and async.** The procedures are spiritlevel's; this crate
  supplies async I/O to them and adds no bring-up logic, so the two cannot diverge.
- **Interfaces owned by consumers.** The SCDC side is the crate's own small trait, not a
  client's whole register map; culvert-async's adapter is an optional convenience.
- **`Send` where the parts allow it.** Every trait has a `Send` twin, and the driver's
  futures are `Send` whenever its parts' are, checked at compile time.
- **Stopped by a flag, not a drop.** Every procedure ends in a defined state, which a
  dropped future cannot give.
- **Type reuse over duplication.** Settings, outcomes, events and errors are spiritlevel's,
  re-exported; there is no parallel type hierarchy.
- **No unsafe code.** `#![forbid(unsafe_code)]`.

---

## What comes next

What is under consideration is on the [roadmap](roadmap.md).
