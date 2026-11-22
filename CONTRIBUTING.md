# Contributing to spiritlevel-async

Thanks for your interest in contributing. This document covers the basics.

## Getting started

Relevant docs for contributors:

- [`doc/setup.md`](doc/setup.md) — build, test, MSRV and coverage commands
- [`doc/testing.md`](doc/testing.md) — what the tests check and how CI runs them
- [`doc/architecture.md`](doc/architecture.md) — role, scope, the async driver and its
  traits, `Send` futures, cancellation, and design principles
- [`doc/roadmap.md`](doc/roadmap.md) — what is under consideration

## Issues and pull requests

**Open an issue first** if you're unsure whether something is a bug or if you want to
discuss a change before implementing it. For small, self-contained fixes a PR on its own
is fine.

- Bug reports: give the procedure called and its inputs, the `TmdsEvent`s it recorded, and
  what you expected, ideally with the HDMI source implementation whose behaviour you
  expected it to match. A procedure behaving differently from spiritlevel's sync driver
  on the same I/O is a bug here; a procedure that is wrong in both is spiritlevel's.
- Feature requests: a brief description of what you need and why is enough to start a
  conversation.
- PRs: keep them focused. One logical change per PR makes review faster and keeps history
  readable.

## Coding standards

- Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before pushing.
- Public items need rustdoc comments (`cargo rustdoc --all-features -- -D warnings` must pass).
- Follow the existing patterns in the codebase — see
  [`doc/architecture.md`](doc/architecture.md) for the design principles behind them.
- `#![forbid(unsafe_code)]` is enforced; no unsafe code.
- Keep `no_std` compatibility without `alloc`. The procedures, their configuration and
  their outcome types are spiritlevel's, defined once there; this crate adds the async
  driver and its traits, and changes no procedure.
- Every async trait has a `Send` twin, and the driver's futures are `Send` when every part
  is one; the compile-time `Send` test covers each new method.
- This crate makes no claim about HDMI behaviour of its own: a change to what a procedure
  does, its settings or its defaults belongs in spiritlevel, where each such claim cites at
  least two independent public implementations.

## Commit and PR expectations

- Write commit messages in the imperative mood ("Add support for …", not "Added …").
- Keep commits logically atomic. A PR that touches three unrelated things should be three
  commits (or three PRs).
- Tests are expected for new async driver logic (the procedures themselves are
  spiritlevel's, and tested there); see [`doc/testing.md`](doc/testing.md) for the pattern.
- CI must be green before a PR can merge: fmt, clippy, docs, the tests in every feature
  set, the bare-metal `no_std` builds, the same tests and builds on the MSRV (Rust 1.85),
  and coverage must not drop more than 0.1 percentage points below the baseline (stored in
  `.coverage-baseline`).

## Publishing and downstream dependencies

spiritlevel-async builds on [spiritlevel](https://github.com/ashlar-display/spiritlevel)
and [hdmi-hal-async](https://github.com/ashlar-display/hdmi-hal-async), and with its
`culvert-async` feature on [culvert-async](https://github.com/ashlar-display/culvert-async),
and uses their types in its API (spiritlevel's configuration, event and outcome types, the
async PHY, clock and delay traits, and culvert-async's client). Their versions are
therefore part of spiritlevel-async's API: moving to a breaking release of one of them is
a breaking release of spiritlevel-async, unless the version requirement keeps accepting
the old release as well.

The `culvert-async` adapter returns culvert-async's errors in spiritlevel's `CulvertError`,
which wraps culvert's `ScdcError`, so spiritlevel's culvert requirement and culvert-async's
must resolve to the same culvert release series. The culvert-async requirement here
therefore never accepts a culvert-async built on a culvert series that spiritlevel does not
accept: moving to a new culvert series takes a spiritlevel release on it first, then a
release of this crate requiring both.

Other
[Ashlar crates](https://ashlar-display.org/crates.html) depend on spiritlevel-async as a
versioned crate dependency. Changes that add or modify public API must be merged, tagged,
and published to crates.io before downstream crates can adopt them. The workflow is:

1. Merge and tag a release in this repo (`vX.Y.Z`).
2. The publish CI workflow uploads the new version to crates.io.
3. Update the version constraint in the downstream crate's `Cargo.toml`.

## Review process

PRs are reviewed on a best-effort basis. Expect feedback within a few days; if you haven't
heard back in a week feel free to ping the thread. Reviews aim to be constructive — if
something needs to change, the reviewer will explain why. Approval from the maintainer is
required to merge.

## Code of Conduct

This project follows the [Contributor Covenant 3.0](https://github.com/ashlar-display/.github/blob/HEAD/CODE_OF_CONDUCT.md),
the code of conduct of every [Ashlar](https://ashlar-display.org) repository. Please read it
before participating.
