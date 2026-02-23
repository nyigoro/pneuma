# Architecture

## Broker

The broker receives runtime signals and selects the active engine.
In early scaffolding this logic is intentionally simple and deterministic.

### Navigate Options (opts_json)

The broker forwards the `opts_json` string to engine `navigate()` calls without
modification. The JS runtime consumes these options in `ghost_shim.js`.

Current fields:
- `stealth_level`: `0 | 1`
  - `0` = no stealth patches (default)
  - `1` = post-navigate JS identity normalization

## Ladybird Embedding

`shims/ladybird_headless.h` and `shims/ladybird_headless.cpp` define the first C++ boundary.
`crates/pneuma-engines/src/ladybird/bridge.rs` is where Rust-side FFI integration will expand.
