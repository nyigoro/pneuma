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

## Servo Smoke Validation (CI)
To run the Servo smoke validation workflow:

1. Add `SERVO_WEBDRIVER_URL` at:
   `https://github.com/nyigoro/pneuma/settings/secrets/actions`
2. Open:
   `https://github.com/nyigoro/pneuma/actions`
3. Manually run the `servo-smoke` workflow (`workflow_dispatch`).

The workflow runs:

```bash
cargo test -p pneuma-core --test servo_smoke -- --ignored --nocapture
```

Expected success signal: output containing `Example Domain`.

Logs are always uploaded as artifact `servo-smoke.log` (retained 7 days), even on failure.
Download it from the workflow run summary and inspect:

- session creation response body (`pneuma_engines` debug logs)
- first evaluate response body (`pneuma_engines` debug logs)

## Servo WebDriver Local Run (Windows)

If Servo is installed via the Windows tech demo bundle, the binary is at:

`C:\Program Files\Servo\Servo Tech Demo\servo.exe`

Start the WebDriver on port 4444:

```powershell
& "C:\Program Files\Servo\Servo Tech Demo\servo.exe" --webdriver=4444
```

## Running the Escalation Smoke Test (WSL + Windows Servo)

The `escalation_to_ladybird_smoke` test requires both a live Servo instance
and a Ladybird build. On Windows with WSL:

1. Start Servo on Windows:
   ```powershell
   servo.exe --webdriver=4444
   ```

2. Allow WSL to reach port 4444 (run once as Administrator):
   ```powershell
   New-NetFirewallRule `
     -DisplayName "Servo WebDriver WSL" `
     -Direction Inbound -Protocol TCP `
     -LocalPort 4444 -RemoteAddress 172.16.0.0/12 `
     -Action Allow -Profile Any -EdgeTraversalPolicy Allow
   ```

3. Run the test from WSL:
   ```bash
   WSL_HOST=$(grep nameserver /etc/resolv.conf | awk '{print $2}')
   CC=clang-20 CXX=clang++-20 \
   LADYBIRD_BUILD_DIR=/root/ladybird16b-dryrun/ladybird/Build/debug-clang20 \
   SERVO_WEBDRIVER_URL=http://$WSL_HOST:4444 \
   cargo test -p pneuma-core --test escalation_to_ladybird_smoke \
     --features ladybird -- --ignored --nocapture
   ```

## Running Ladybird Transport Proxy Smoke

This smoke test validates Ladybird proxy plumbing through the broker transport
profile path. It uses an in-test local recording proxy and checks that Ladybird
traffic reaches that proxy endpoint.

Prerequisites:
- `LADYBIRD_BUILD_DIR` points to a usable Ladybird build.
- Build host has the Ladybird toolchain (`cmake`, `clang-20`, `ninja`, etc.).

Run:

```bash
LADYBIRD_BUILD_DIR=/path/to/ladybird/Build/debug-clang20 \
cargo test -p pneuma-core --test transport_proxy_ladybird_smoke \
  --features "ladybird transport-stealth" -- --ignored --nocapture
```
