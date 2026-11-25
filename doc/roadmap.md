# Roadmap

What comes next for `spiritlevel-async`. The procedures and their settings follow
spiritlevel: each change there reaches this crate in its next release. The changelog is the
record of what shipped.

## Planned

- **`SendTmdsScdc` for culvert-async's `SendScdc<T>`,** the wrapper culvert-async plans
  around its client for code bounded by a `Send` twin, once culvert-async releases it: one
  client type for FRL training and TMDS bring-up in code bounded by the twins. Additive, with
  the culvert-async requirement raised to that release. Until then, code bounded by
  `SendTmdsScdc` uses a local forwarding wrapper ([architecture](architecture.md), "The
  culvert-async adapter").

## Under consideration

- **Return-type notation instead of the `Send` twins.** Once callers can bound the plain
  traits' futures (`S: TmdsScdc<read_tmds_config(..): Send>`) on stable Rust, the twins can
  go, in a breaking release, together with hdmi-hal-async's.
- **Conformance checks for `TmdsScdc` implementations,** run against a register-level
  simulated sink: each method's reads and writes, `clear_flt_update` leaving the other
  `Update_0` flags set as spiritlevel's `TmdsScdc` requires, and errors passing their kind
  through.
- **`defmt` support** for the events and outcomes, decided across the stack's crates.
