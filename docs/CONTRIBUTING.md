# Contributing to Pneuma

Pneuma needs three kinds of contributors and the work is cleanly separated:

## Track 1: Engine Specialists (C++ / Browser internals)
You own `vendor/ladybird/` and `shims/`.
Your job: strip the GUI from Ladybird's LibWeb, maintain the
`ladybird_headless.h` interface, and fix build breakage when
upstream Ladybird changes.
Skills needed: C++23, CMake, browser engine internals.
Start here: `docs/ARCHITECTURE.md#ladybird-embedding`

## Track 2: Systems Engineers (Rust)
You own `crates/pneuma-broker/`, `crates/pneuma-network/`,
and `crates/pneuma-engines/`.
Your job: implement the Confidence Scorer, state migration,
and the NetworkInterceptor.
Skills needed: async Rust, tokio, unsafe FFI, systems programming.
Start here: `docs/ARCHITECTURE.md#broker`

## Track 3: Patch & Stealth Contributors (JavaScript / Security)
You own `patches/` and `profiles/`.
Your job: write polyfill patches for Servo gaps, maintain
stealth identity profiles, and document detection vectors.
Skills needed: JavaScript, browser fingerprinting, TLS basics.
Start here: `patches/registry.toml`

No Rust required for Track 3. If you can write a polyfill
and identify the error pattern that triggers it,
you can contribute.
