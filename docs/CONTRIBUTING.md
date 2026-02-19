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

## Week 8 Live Validation (Servo WebDriver)
To run the Week 8 smoke validation workflow:

1. Add `SERVO_WEBDRIVER_URL` at:
   `https://github.com/nyigoro/pneuma/settings/secrets/actions`
2. Open:
   `https://github.com/nyigoro/pneuma/actions`
3. Manually run the `week8-servo-smoke` workflow (`workflow_dispatch`).

The workflow runs:

```bash
cargo test -p pneuma-core --test week7_servo_smoke -- --ignored --nocapture
```

Expected success signal: output containing `Example Domain`.

Logs are always uploaded as artifact `week8-servo-smoke.log` (retained 7 days), even on failure.
Download it from the workflow run summary and inspect:

- session creation response body (`pneuma_engines` debug logs)
- first evaluate response body (`pneuma_engines` debug logs)
