# spiritlevel-async

[![CI](https://github.com/ashlar-display/spiritlevel-async/actions/workflows/ci.yml/badge.svg)](https://github.com/ashlar-display/spiritlevel-async/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/spiritlevel-async.svg)](https://crates.io/crates/spiritlevel-async)
[![docs.rs](https://docs.rs/spiritlevel-async/badge.svg)](https://docs.rs/spiritlevel-async)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rustc-1.85+-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)
[![SLSA Level 2](https://slsa.dev/images/gh-badge-level2.svg)](https://slsa.dev)
[![trusted publishing only](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fcrates.io%2Fapi%2Fv1%2Fcrates%2Fspiritlevel-async&query=%24.crate.trustpub_only&label=trusted%20publishing%20only)](#verifying-releases)

> **Part of [Ashlar](https://ashlar-display.org)** — a display connection stack in Rust,
> HDMI 2.1 first: from EDID parsing through mode negotiation to FRL link training.
> See [all crates](https://ashlar-display.org/crates.html).

Async TMDS link bring-up for HDMI sources.

`spiritlevel-async` is the async companion to
[spiritlevel](https://github.com/ashlar-display/spiritlevel). It has no procedures of its
own: it runs spiritlevel's — bring-up, tear-down, the reconnect check after a hotplug, and
the quick restore — over an async SCDC implementation and
[hdmi-hal-async](https://github.com/ashlar-display/hdmi-hal-async)'s PHY, clock and delay,
`.await`ing each operation, so a bring-up yields to the executor wherever the I/O or a
wait does. Because the procedures are written once, sync and async bring-up take the same
steps and record the same events given the same I/O, and every fix reaches both.

## Status

The `TmdsLink` driver runs all four of spiritlevel's procedures, over the async `TmdsScdc`
trait and its `Send` twin, with the culvert-async adapter behind a feature. What is under
consideration is on the [roadmap](doc/roadmap.md).

## Usage

```sh
cargo add spiritlevel-async
```

Choose the link with spiritlevel's `TmdsLinkParams::choose` (re-exported here), then bring
it up over the parts: the sink's SCDC (`TmdsScdc`, implemented for culvert-async's client
by the `culvert-async` feature), an async `TmdsPhy`, a `Clock` and an async `Delay`. Here
the parts are doubles (hidden), each run answering at once, so a one-poll `block_on` runs
the example; on hardware, the executor awaits it:

```rust
// spiritlevel-async re-exports the spiritlevel and hdmi-hal-async types its API uses, so
// it is the only dependency.
use spiritlevel_async::{
    BringUpConfig, BringUpOutcome, Cancel, CheckResult, Clock, ColorBitDepth, ColorFormat,
    Delay, SinkPresence, SinkTmdsFacts, SourceTmdsFacts, TearDownConfig, TmdsLink,
    TmdsLinkParams, TmdsPhy, TmdsPolicy, TmdsScdc, TmdsVideo,
};
# use core::cell::Cell;
# use core::sync::atomic::AtomicBool;
# use core::time::Duration;
# use spiritlevel_async::{
#     DdcErrorKind, ScramblingStatus, SinkTmdsConfig, TmdsConfigReading, TmdsPhyCapabilities,
#     TmdsPhyConfig, Timestamp, UpdateError,
# };

/// How a bring-up went.
#[derive(Debug, PartialEq)]
enum Shown {
    /// Up, and whether the sink reports scrambled TMDS.
    Up { scrambling_seen: bool },
    /// Stopped by the cancel flag.
    Cancelled,
    /// A step failed; the bring-up's own tear-down followed.
    Failed,
    /// No link for this video on this sink and source.
    Unsupported,
}

/// Brings up 4K60 RGB over TMDS, and stops it again once it is up.
async fn show<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay, X: Cancel>(
    tmds: &mut TmdsLink<S, P, K, D, X>,
    sink: SinkTmdsFacts,
) -> Shown {
    // The link is chosen for the rate the video gives; bring-up refuses any other.
    let video = TmdsVideo::new(594_000, ColorBitDepth::Depth8, ColorFormat::Rgb444);
    let Some(rate_khz) = video.character_rate_khz() else {
        return Shown::Unsupported; // a video TMDS cannot carry
    };
    let source = SourceTmdsFacts::from(&tmds.phy().capabilities());
    let Ok(link) = TmdsLinkParams::choose(rate_khz, sink, source, TmdsPolicy::DEFAULT) else {
        return Shown::Unsupported; // a rate this sink and source cannot carry
    };

    match tmds.bring_up(&link, video, &BringUpConfig::DEFAULT).await {
        Ok(BringUpOutcome::Up(up)) => {
            // What went wrong without stopping the bring-up, if anything.
            for _warning in up.warnings.iter_warnings() {}
            let _ = tmds
                .tear_down(&link, &TearDownConfig::DEFAULT, SinkPresence::Present)
                .await;
            Shown::Up {
                scrambling_seen: up.check.result == CheckResult::Set,
            }
        }
        Ok(_cancelled) => Shown::Cancelled, // the cancel flag (`with_cancel`)
        Err(_error) => Shown::Failed,       // which step, and how the tear-down went
    }
}
# // Doubles that answer at once: a sink with `TMDS_Config` and `Scrambling_Status`, a PHY
# // that fails `configure` when told to, and one value as clock and delay.
# struct Sink { tmds_config: SinkTmdsConfig }
# impl TmdsScdc for Sink {
#     type Error = DdcErrorKind;
#     async fn write_tmds_config(&mut self, c: SinkTmdsConfig) -> Result<(), DdcErrorKind> {
#         self.tmds_config = c;
#         Ok(())
#     }
#     async fn update_tmds_config(&mut self, c: SinkTmdsConfig) -> Result<(), UpdateError<DdcErrorKind>> {
#         self.tmds_config = c;
#         Ok(())
#     }
#     async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, DdcErrorKind> {
#         let c = self.tmds_config;
#         Ok(TmdsConfigReading::new(c, u8::from(c.scrambling_enable) | u8::from(c.high_tmds_clock_ratio) << 1))
#     }
#     async fn read_scrambling_status(&mut self) -> Result<ScramblingStatus, DdcErrorKind> {
#         let set = self.tmds_config.scrambling_enable;
#         Ok(ScramblingStatus::new(set, u8::from(set)))
#     }
#     async fn read_sink_version(&mut self) -> Result<u8, DdcErrorKind> { Ok(1) }
#     async fn write_source_version(&mut self, _: u8) -> Result<(), DdcErrorKind> { Ok(()) }
#     async fn clear_frl_rate(&mut self) -> Result<(), DdcErrorKind> { Ok(()) }
#     async fn clear_flt_update(&mut self) -> Result<bool, UpdateError<DdcErrorKind>> { Ok(false) }
# }
# struct Phy { fail: bool }
# impl TmdsPhy for Phy {
#     type Error = ();
#     fn capabilities(&self) -> TmdsPhyCapabilities {
#         TmdsPhyCapabilities::new(600_000).with_scrambling(true)
#     }
#     async fn configure(&mut self, _: TmdsPhyConfig) -> Result<(), ()> {
#         if self.fail { Err(()) } else { Ok(()) }
#     }
#     async fn set_scrambling(&mut self, _: bool) -> Result<(), ()> { Ok(()) }
#     async fn start_output(&mut self) -> Result<(), ()> { Ok(()) }
#     async fn stop_output(&mut self) -> Result<(), ()> { Ok(()) }
# }
# #[derive(Default)]
# struct Time(Cell<u64>);
# impl Clock for &Time {
#     fn now(&self) -> Timestamp { Timestamp::from_micros(self.0.get()) }
# }
# impl Delay for &Time {
#     async fn delay(&mut self, d: Duration) {
#         self.0.set(self.0.get() + u64::try_from(d.as_micros()).unwrap());
#     }
# }
# /// Polls a future whose I/O completes at once.
# fn block_on<F: core::future::Future>(future: F) -> F::Output {
#     let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
#     match core::pin::pin!(future).poll(&mut cx) {
#         core::task::Poll::Ready(output) => output,
#         core::task::Poll::Pending => unreachable!("the doubles never wait"),
#     }
# }
# fn sink() -> Sink { Sink { tmds_config: SinkTmdsConfig::new(false, false) } }

// A sink with SCDC that takes scrambling, without low-rate scrambling.
let facts = SinkTmdsFacts::new(true, true, false);
let time = Time::default();

let mut tmds = TmdsLink::new(sink(), Phy { fail: false }, &time, &time);
assert_eq!(block_on(show(&mut tmds, facts)), Shown::Up { scrambling_seen: true });

// A flag already set: the bring-up stops at entry.
let unplugged = AtomicBool::new(true);
let mut tmds =
    TmdsLink::new(sink(), Phy { fail: false }, &time, &time).with_cancel(&unplugged);
assert_eq!(block_on(show(&mut tmds, facts)), Shown::Cancelled);

// A PHY that refuses its configuration.
let mut tmds = TmdsLink::new(sink(), Phy { fail: true }, &time, &time);
assert_eq!(block_on(show(&mut tmds, facts)), Shown::Failed);
```

The procedures, their settings, outcomes and events are spiritlevel's: see
[spiritlevel's README](https://github.com/ashlar-display/spiritlevel#usage) and its
architecture document for what each step does and why. Each procedure has a `_with_events`
form that calls a closure with every step, timestamped from the caller's clock;
`MAX_BRING_UP_EVENTS` and its siblings bound how many, so a caller without `alloc` sizes a
channel or buffer from them. The closure runs between `.await`s and cannot await: it hands
each event on, with a channel's `try_send` for example.

## Stopping a procedure

Stop a running procedure with its cancel flag, never by dropping its future. The flag
(`Cancel`, implemented for `AtomicBool` and references to one) is given with `with_cancel`,
and each procedure reads it where spiritlevel says:

- **`bring_up`** reads it at entry, before each SCDC step, before each slice of a wait and
  before each read of the scrambling check. Seen at entry, the bring-up ends
  `BringUpOutcome::Cancelled` before any I/O, with nothing to undo (`Cleanup::Skipped`);
  seen later, it ends `Cancelled` after the tear-down it carries.
- **`quick_restore`** reads it only before each read of its scrambling check: its write and
  its scrambler call always run, and on a plain link, which has no check, it is not read.
  A cancelled check is `CheckResult::Cancelled`, with the link left running.
- **`tear_down`** and **`check_reconnect`** never read it: the tear-down is the recovery,
  and the check is one read.

No procedure clears the flag: reset it before the next call, or the next `bring_up` ends
`Cancelled` at entry.

Dropping the future instead — losing a `select!`, hitting a `timeout`, aborting the task —
stops the procedure at whichever `.await` it was suspended on, and a dropped future cannot
run its tear-down: the sink may hold a new `TMDS_Config` and the PHY may be transmitting.
After a dropped `bring_up` or `quick_restore`, call `tear_down` with the same link before
anything else (a `quick_restore` may simply be called again); after a dropped
`tear_down`, call it again; a dropped `check_reconnect` needs nothing. The procedures bound
their own timing, so a timeout around them adds nothing, provided each call of the parts
completes in bounded time.

## Multi-threaded executors

The plain traits promise nothing about their futures, so code generic over them cannot
spawn a procedure on a multi-threaded executor. For that, implement the `Send` twins:
`SendTmdsScdc`, and hdmi-hal-async's `SendTmdsPhy` and `SendDelay` (re-exported). Every
procedure's future is then `Send` in generic code bounded by the twins alone, with a clock
and a flag that are `Send` (they need not be `Sync`) and, for the `_with_events` forms, a
`Send` closure:

```rust
use core::sync::atomic::{AtomicBool, Ordering};
use spiritlevel_async::{
    BringUpConfig, Clock, SendDelay, SendTmdsPhy, SendTmdsScdc, TmdsLink, TmdsLinkParams,
    TmdsVideo,
};
# // A stand-in with tokio::spawn's bound: the future must be `Send + 'static`.
# mod tokio {
#     pub fn spawn<F: core::future::Future + Send + 'static>(_future: F) {}
# }

/// Set by the task that watches the hotplug line; the bring-up stops at its next check.
static UNPLUGGED: AtomicBool = AtomicBool::new(false);

fn spawn_bring_up<S, P, K, D>(
    scdc: S,
    phy: P,
    clock: K,
    delay: D,
    link: TmdsLinkParams,
    video: TmdsVideo,
) where
    S: SendTmdsScdc + 'static,
    P: SendTmdsPhy + 'static,
    K: Clock + Send + 'static,
    D: SendDelay + 'static,
{
    UNPLUGGED.store(false, Ordering::Relaxed); // the flag is the caller's to reset
    tokio::spawn(async move {
        let mut tmds = TmdsLink::new(scdc, phy, clock, delay).with_cancel(&UNPLUGGED);
        let _outcome = tmds.bring_up(&link, video, &BringUpConfig::DEFAULT).await;
    });
}
```

Bound the code by the twins alone: adding `+ TmdsScdc`, `+ TmdsPhy` or `+ Delay`, which the
twins imply anyway, makes the compiler use that bound instead of the blanket
implementation, and the futures are no longer known to be `Send`. With a twin and its plain
trait both in scope, a method call on a twin implementation is ambiguous: import the trait
your code is bounded by.

On **embassy**, the clock and delay come from `embassy-time`, events go into an
`embassy_sync` channel with `try_send`, and the task that watches the hotplug line sets a
`static` flag as above. On **tokio**, the clock is measured from a start
`tokio::time::Instant`, on the same timebase as `sleep`, and a flag shared as an
`Arc<AtomicBool>` is passed as `with_cancel(&*flag)` inside the task. On Linux the stack's
backend is synchronous: run spiritlevel's sync `TmdsLink` on a thread of its own there.
[`doc/architecture.md`](doc/architecture.md), "On embassy and tokio", has the details.

## The culvert-async adapter

With the `culvert-async` feature, `TmdsScdc` is implemented for culvert-async's client
`Scdc<T>` over any async `ScdcTransport`, and for a mutable borrow of it, so one client
serves SCDC setup, FRL training and TMDS bring-up in turn. Errors are spiritlevel's
`CulvertError`, the type spiritlevel's own culvert adapter returns. The client has methods
of its own named like five of the trait's (`read_tmds_config`, `write_tmds_config`,
`update_tmds_config`, `read_sink_version`, `write_source_version`), which a method call
picks over the trait's: call the trait's as `TmdsScdc::read_tmds_config(&mut scdc)`. In
code generic over `T: SendScdcTransport` that names `Scdc<T>`, the futures are `Send`
already. Code bounded
by `S: SendTmdsScdc` cannot take `Scdc<T>` itself; until culvert-async's planned `Send`
wrapper is supported (see the [roadmap](doc/roadmap.md)), it wraps the client in a local
type that implements `SendTmdsScdc` by forwarding to `Scdc<T>`.

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `display-types` | yes | spiritlevel's `From` conversions from display-types' `HdmiForumSinkCap` into the sink facts |
| `culvert-async` | no | `TmdsScdc` for [culvert-async](https://github.com/ashlar-display/culvert-async)'s client and a borrow of it, and culvert-async's types re-exported |

## `no_std` builds

`spiritlevel-async` is `#![no_std]` without `alloc` in every build, and
`#![forbid(unsafe_code)]`. It depends on no executor or runtime crate.

## Stack position

`spiritlevel-async` drives [spiritlevel](https://github.com/ashlar-display/spiritlevel)'s
procedures, over [hdmi-hal-async](https://github.com/ashlar-display/hdmi-hal-async)'s
async TMDS PHY and delay and its clock, and, behind a feature,
[culvert-async](https://github.com/ashlar-display/culvert-async)'s SCDC client.
[plumbob-async](https://github.com/ashlar-display/plumbob-async), which trains FRL links,
is its counterpart beside it.

## Out of scope

The procedures themselves (spiritlevel's), choosing the link, the hardware, powering the
PHY, retries, hotplug handling, and an executor. See
[`doc/architecture.md`](doc/architecture.md).

## Documentation

- [`doc/architecture.md`](doc/architecture.md) — role, scope, the traits and the driver,
  `Send` futures, cancellation, embassy and tokio, and design principles
- [`doc/roadmap.md`](doc/roadmap.md) — what is under consideration
- [`doc/setup.md`](doc/setup.md) — build, test, MSRV and coverage commands
- [`doc/testing.md`](doc/testing.md) — what the tests check and how CI runs them
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — issues, pull requests and coding standards

## Verifying releases

Releases are built on GitHub Actions and attested with
[SLSA Build Level 2](https://slsa.dev) provenance. To verify a release `.crate` against
its signed provenance, install the [GitHub CLI](https://cli.github.com/) and run:

```sh
gh attestation verify spiritlevel-async-X.Y.Z.crate --repo ashlar-display/spiritlevel-async
```

The attested `.crate` is attached to each
[GitHub release](https://github.com/ashlar-display/spiritlevel-async/releases).

The first release, 0.1.0, is uploaded by the publish workflow with a crates.io API token
scoped to publishing this new crate, because trusted publishing can only be set up for a
crate already on crates.io; the token is revoked afterwards. Later releases are uploaded
through crates.io
[trusted publishing](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html):
the publish workflow obtains a short-lived token via OpenID Connect instead of using a
stored API token. Once trusted publishing is set up, the crate is switched to trusted
publishing only, so that crates.io rejects uploads made with an API token for it; the badge
above shows whether that setting is on. Each version's publishing details, including
the workflow run and commit, are available from the crates.io API:

```sh
curl -A "your-tool (contact)" https://crates.io/api/v1/crates/spiritlevel-async/X.Y.Z
# see .version.trustpub_data
```

## License

[MPL-2.0](LICENSE)
