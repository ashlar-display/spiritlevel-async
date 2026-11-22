# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Internal

- CI: format, clippy and tests with default features, without them and with
  `culvert-async`; docs with every feature and `-D warnings`, as docs.rs builds them;
  `no_std` builds for the bare `thumbv7em-none-eabi` target in the same feature sets; a
  read-only token except where the coverage job opens its PR. On `main`, CI fails when
  `Cargo.toml` has a `[patch]` section.
- MSRV check: `cargo msrv verify` runs the tests per feature set and the bare `no_std`
  build on the `rust-version` in `Cargo.toml` (1.85).
- Coverage ratchet: CI measures line coverage, the `culvert-async` adapter included,
  against `.coverage-baseline` and opens a PR when it rises.
- Weekly RustSec audit, a weekly docs build as docs.rs does it (nightly, `--cfg docsrs`),
  and Dependabot for the workflow pins and `Cargo.lock`.
- Release workflows (release-prep, release-tag, publish) as in the other crates, with a
  first-release path: release-prep releases the version in `Cargo.toml` when the crate is
  not on crates.io yet, and publish uses a first-publish token where trusted publishing is
  not yet possible. publish's verify and MSRV jobs mirror CI and fail on a `[patch]`.
