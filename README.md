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

Built: the async `TmdsScdc` trait and its `Send` twin, the `TmdsLink` driver with all four
procedures, the culvert-async adapter, and the re-exports. Not yet released; see the
[roadmap](doc/roadmap.md) for what is under consideration.

## Usage

```sh
cargo add spiritlevel-async
```

Choose the link with spiritlevel's `TmdsLinkParams::choose` (re-exported here), then bring
it up over the parts: the sink's SCDC (`TmdsScdc`, implemented for culvert-async's client
by the `culvert-async` feature), an async `TmdsPhy`, a `Clock` and an async `Delay`:

```rust
// spiritlevel-async re-exports the spiritlevel and hdmi-hal-async types its API uses, so
// it is the only dependency.
use spiritlevel_async::{
    BringUpConfig, BringUpOutcome, CheckResult, Clock, ColorBitDepth, ColorFormat, Delay,
    SinkPresence, SinkTmdsFacts, SourceTmdsFacts, TearDownConfig, TmdsLink, TmdsLinkParams,
    TmdsPhy, TmdsPolicy, TmdsScdc, TmdsVideo,
};

/// Brings up 4K60 RGB over TMDS, then stops it again.
async fn show<S: TmdsScdc, P: TmdsPhy, K: Clock, D: Delay>(
    scdc: S,
    phy: P,
    clock: K,
    delay: D,
    sink: SinkTmdsFacts,
) {
    // The link is chosen for the rate the video gives; bring-up refuses any other.
    let video = TmdsVideo::new(594_000, ColorBitDepth::Depth8, ColorFormat::Rgb444);
    let Some(rate_khz) = video.character_rate_khz() else {
        return; // a video TMDS cannot carry
    };
    let source = SourceTmdsFacts::from(&phy.capabilities());
    let Ok(link) = TmdsLinkParams::choose(rate_khz, sink, source, TmdsPolicy::DEFAULT) else {
        return; // a rate this sink and source cannot carry
    };

    let mut tmds = TmdsLink::new(scdc, phy, clock, delay);
    match tmds.bring_up(&link, video, &BringUpConfig::DEFAULT).await {
        Ok(BringUpOutcome::Up(up)) => {
            // Whether the sink reports scrambled TMDS, and what went wrong without
            // stopping the bring-up.
            let _seen = up.check.result == CheckResult::Set;
            for _warning in up.warnings.iter_warnings() {}
        }
        Ok(_cancelled) => {} // the cancel flag (`with_cancel`); never without one
        Err(_error) => {}    // the step that failed, and the tear-down that followed
    }
    let _ = tmds
        .tear_down(&link, &TearDownConfig::DEFAULT, SinkPresence::Present)
        .await;
}
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
(`Cancel`, implemented for `AtomicBool` and references to one) is given with `with_cancel`
and read where spiritlevel says: at entry, before each SCDC step, before each slice of a
wait and before each read of the scrambling check. A bring-up it stops ends
`BringUpOutcome::Cancelled`, after the tear-down it carries. No procedure clears the flag:
reset it before the next call, or that call ends `Cancelled` at entry.

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
`CulvertError`, the type spiritlevel's own culvert adapter returns. In code generic over
`T: SendScdcTransport` that names `Scdc<T>`, the futures are `Send` already; code bounded
by `S: SendTmdsScdc` takes the `SendScdc<T>` wrapper instead.

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `display-types` | yes | spiritlevel's `From` conversions from display-types' `HdmiForumSinkCap` into the sink facts |
| `culvert-async` | no | `TmdsScdc` for [culvert-async](https://github.com/ashlar-display/culvert-async)'s client and a borrow of it, `SendTmdsScdc` for `SendScdc`, and culvert-async's types re-exported |

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
stored API token, and crates.io rejects uploads made with an API token for this crate; the
badge above shows whether that setting is on. Each version's publishing details, including
the workflow run and commit, are available from the crates.io API:

```sh
curl -A "your-tool (contact)" https://crates.io/api/v1/crates/spiritlevel-async/X.Y.Z
# see .version.trustpub_data
```

## License

[MPL-2.0](LICENSE)
