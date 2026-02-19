# Architecture

## Broker

The broker receives runtime signals and selects the active engine.
In early scaffolding this logic is intentionally simple and deterministic.

## Ladybird Embedding

`shims/ladybird_headless.h` and `shims/ladybird_headless.cpp` define the first C++ boundary.
`crates/pneuma-engines/src/ladybird/bridge.rs` is where Rust-side FFI integration will expand.
