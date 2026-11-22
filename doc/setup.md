# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/spiritlevel-async.git
cd spiritlevel-async
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --features culvert-async -- -D warnings
cargo rustdoc --all-features -- -D warnings
```

## Running tests

```sh
cargo test                           # default features (display-types)
cargo test --no-default-features     # without display-types' From impls
cargo test --features culvert-async  # with the adapter for culvert-async's SCDC client
```

The README's examples run as doctests with the others.

## `no_std` builds

The crate is `no_std` without `alloc` in every build. CI checks it on a bare embedded
target (`thumbv7em-none-eabi`, firmware without a heap) and on the UEFI targets
(`x86_64-unknown-uefi` and `aarch64-unknown-uefi`, firmware drivers):

```sh
rustup target add thumbv7em-none-eabi x86_64-unknown-uefi aarch64-unknown-uefi
for target in thumbv7em-none-eabi x86_64-unknown-uefi aarch64-unknown-uefi; do
  cargo build --target "$target"
  cargo build --no-default-features --target "$target"
  cargo build --features culvert-async --target "$target"
done
```

## Checking the MSRV

The MSRV is `rust-version` in `Cargo.toml`. CI checks it with
[`cargo-msrv`](https://github.com/foresterre/cargo-msrv), which runs the checks on that
toolchain:

```sh
cargo install cargo-msrv --version 0.19.6 --locked
cargo msrv verify -- cargo test --locked
cargo msrv verify -- cargo test --locked --no-default-features
cargo msrv verify -- cargo test --locked --features culvert-async
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --target thumbv7em-none-eabi
cargo msrv verify --target x86_64-unknown-uefi \
  -- cargo build --locked --no-default-features --target x86_64-unknown-uefi
cargo msrv verify --target aarch64-unknown-uefi \
  -- cargo build --locked --no-default-features --target aarch64-unknown-uefi
```

## Measuring coverage

Coverage requires [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```sh
cargo install cargo-llvm-cov
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report
cargo llvm-cov --no-report --features culvert-async
cargo llvm-cov report
```

The second run covers the `culvert-async` adapter; the report merges both, as CI does.

The current baseline is stored in `.coverage-baseline`. CI fails if coverage drops more
than 0.1 percentage points below it. On pushes to `main` or `develop`, an improvement automatically opens
a `ci/coverage-ratchet` PR to commit the new baseline.

Rust versions count lines differently, so the same code can measure differently on two
versions. Measure the baseline with the Rust version CI's `stable` uses
(`cargo +<version> llvm-cov …`), or CI can fail on a baseline that a different local
version produced.

