# Testing

What the tests check and how CI runs them. The procedures themselves — their steps,
settings, failures, cancellation and event bounds — are spiritlevel's and tested there;
the tests here check what this crate adds: the async driver, its traits, the culvert-async
adapter, and that sync and async bring-up agree.

## The async driver (`src/link/tests.rs`)

Scripted async doubles of `TmdsScdc`, `TmdsPhy` and `Delay` record every call and answer
from a few registers; every other call returns `Pending` once before it completes, so the
procedures run over I/O that really suspends.

- **Each procedure through the driver:** bring-up on a plain and a scrambled link, the
  tear-down, the reconnect check and the quick restore, with their outcomes and the parts'
  state afterwards.
- **Events:** each `_with_events` form passes every event to the callback, within the
  procedure's `MAX_*_EVENTS`.
- **The cancel flag:** set before a call, the bring-up ends `Cancelled` at entry with
  nothing done; set during one, it ends `Cancelled` after its tear-down.
- **The parts between procedures:** `with_cancel`, `into_parts` and the accessors return
  what was given.
- **The `Send` twin through the driver:** an SCDC implementation of `SendTmdsScdc` alone
  drives `TmdsLink` as a `TmdsScdc`, through the blanket implementation.

## The SCDC traits (`src/scdc.rs`)

- **The blanket implementation:** a `SendTmdsScdc` drives `TmdsLink` as a `TmdsScdc`, and
  every call reaches the twin's method.

## The culvert-async adapter (`src/culvert.rs`, `culvert-async` feature)

Over a register-level async `ScdcTransport`:

- **Each method** reads and writes the registers it should, `update_tmds_config` keeping
  the other bits of `TMDS_Config`, and the bytes read reach `TmdsConfigReading::raw` and
  `ScramblingStatus::raw`.
- **Errors:** a transport error's kind passes through `CulvertError`, and a failed read or
  write of a read-modify-write is told apart.
- **`&mut Scdc<T>`** forwards every call to the client.

## `Send` futures (`tests/send.rs`)

Functions generic over `SendTmdsScdc`, `SendTmdsPhy` and `SendDelay` pass the future of
each procedure, plain and with events, through a function that requires `Send`, so the test
compiles only if every procedure could run on a multi-threaded executor. The clock and the
cancel flag are `Send` but not `Sync`, so a change that made the driver hold them through a
shared reference fails to compile. With `culvert-async`, the same holds for `Scdc<T>` and
`&mut Scdc<T>` over a `SendScdcTransport`.

## Sync and async agree (`tests/simulated_sink.rs`, `culvert-async` feature)

Each procedure runs twice against [maquette](https://github.com/ashlar-display/maquette)'s
simulated sink on its simulated clock: once through spiritlevel's sync `TmdsLink` with
spiritlevel's culvert adapter, once through this crate's driver with the culvert-async
adapter, each on a fresh bench with the same sink settings and faults. The two outcomes and
event sequences must be equal, timestamps included, and so must the sink's transcripts. The
cases cover a plain and a scrambled bring-up, a sink that reports scrambling late or never,
a failed `TMDS_Config` write, a detached sink, a cancelled bring-up, the tear-down, the
reconnect check and the quick restore.

## The crate root

- **Every re-export** is named through the crate root (`tests/send.rs`), so a re-export that
  goes missing fails to compile.
- **The README's examples** run as doctests.

## CI

CI runs the tests with default features, without them and with `culvert-async`, builds the
crate for the bare targets (`thumbv7em-none-eabi`, `x86_64-unknown-uefi` and
`aarch64-unknown-uefi`) in the same feature sets, runs the tests in each feature set and
the bare-target builds without default features on the MSRV (Rust 1.85), and measures line
coverage with the adapter included; it fails if coverage drops more than 0.1 percentage
points below the baseline in `.coverage-baseline`. The commands are in [setup](setup.md).
