# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `TmdsScdc`, the async SCDC trait spiritlevel's procedures need, and its `Send` twin
  `SendTmdsScdc`, through whose blanket implementation every `SendTmdsScdc` is a `TmdsScdc`.
- `TmdsLink`, the async driver: spiritlevel's bring-up, tear-down, reconnect check and
  quick restore over an async `TmdsScdc`, `TmdsPhy` and `Delay`, a `Clock` and an optional
  `Cancel` flag, each in a plain and a `_with_events` form, with `with_cancel`,
  `into_parts` and accessors for the parts. Its futures are `Send` when the parts are
  bounded by the `Send` twins.
- The `culvert-async` feature: `TmdsScdc` for culvert-async's `Scdc<T>` and `&mut Scdc<T>`,
  with errors in spiritlevel's `CulvertError`.
- Re-exports of spiritlevel's link parameters, settings, outcomes and events, and of the
  hdmi-hal-async traits and types the API uses (and, with `culvert-async`, culvert-async's
  client, errors and transports).

### Internal

- CI: format, clippy and tests with default features, without them and with
  `culvert-async`; docs with every feature and `-D warnings`, as docs.rs builds them;
  `no_std` builds for the bare `thumbv7em-none-eabi` target and the
  `x86_64-unknown-uefi` and `aarch64-unknown-uefi` targets in the same feature sets; a
  read-only token except where the coverage job opens its PR. On `main`, CI fails when
  `Cargo.toml` has a `[patch]` section.
- MSRV check: `cargo msrv verify` runs the tests per feature set and the `no_std` builds
  without default features for each bare target on the `rust-version` in `Cargo.toml` (1.85).
- Tests: the driver over scripted doubles that suspend, a compile-time check that every
  procedure's future is `Send` over the twins, and, with `culvert-async`, each procedure
  run through spiritlevel's sync driver and this crate's against maquette's simulated
  sink, with equal results, events and transcripts.
- Coverage ratchet: CI measures line coverage, the `culvert-async` adapter included,
  against `.coverage-baseline` and opens a PR when it rises.
- Weekly RustSec audit, a weekly docs build as docs.rs does it (nightly, `--cfg docsrs`),
  and Dependabot for the workflow pins and `Cargo.lock`.
- Release workflows (release-prep, release-tag, publish) as in the other crates, with a
  first-release path: release-prep releases the version in `Cargo.toml` when the crate is
  not on crates.io yet, and publish uses a first-publish token where trusted publishing is
  not yet possible. publish's verify and MSRV jobs mirror CI and fail on a `[patch]`.
