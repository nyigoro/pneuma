# Transport Stealth

Transport stealth routes browser traffic through a profile-specific proxy.
The proxy is responsible for TLS/JA3 impersonation. Engine code only consumes
resolved proxy endpoints.

## Profiles

`TransportStealthProfile` supports:

- `chrome` + version
- `firefox` + version
- `safari` + version
- `edge` + version
- `custom` (`ja3`, `h2_settings`, `alpn`)

Accepted input formats (backward-compatible):

- String: `"Chrome120"`, `"firefox_123"`, `"safari-17"`
- Object (versioned): `{"type":"chrome","version":120}`
- Object (legacy): `{"type":"chrome120"}`
- Object (custom): `{"type":"custom","ja3":"...","h2_settings":[...],"alpn":["h2","http/1.1"]}`

## Environment Variable Resolution

Resolution order per profile:

1. Version-specific:
   - `PNEUMA_TRANSPORT_PROXY_CHROME_<version>`
   - `PNEUMA_TRANSPORT_PROXY_FIREFOX_<version>`
   - `PNEUMA_TRANSPORT_PROXY_SAFARI_<version>`
   - `PNEUMA_TRANSPORT_PROXY_EDGE_<version>`
2. Generic browser fallback:
   - `PNEUMA_TRANSPORT_PROXY_CHROME`
   - `PNEUMA_TRANSPORT_PROXY_FIREFOX`
   - `PNEUMA_TRANSPORT_PROXY_SAFARI`
   - `PNEUMA_TRANSPORT_PROXY_EDGE`
3. Global fallback:
   - `PNEUMA_TRANSPORT_PROXY_URL`

Compatibility aliases (no underscore before version) are also accepted, such
as `PNEUMA_TRANSPORT_PROXY_CHROME120`.

## Smoke Tests

### Proxy plumbing via broker

- Servo path:
  - `cargo test -p pneuma-core --test transport_proxy_usage_smoke --features transport-stealth -- --ignored --nocapture`
- Ladybird path:
  - `cargo test -p pneuma-core --test transport_proxy_ladybird_smoke --features "ladybird transport-stealth" -- --ignored --nocapture`
  - Current behavior: fail-secure fallback to Servo when transport proxy is requested.

### JA3 capture at proxy layer

- `cargo test -p pneuma-transport-stealth --test ja3_impersonation_smoke -- --nocapture`

Optional expected hash vars:

- `PNEUMA_EXPECTED_JA3_CHROME_120`
- `PNEUMA_EXPECTED_JA3_FIREFOX_123`
- `PNEUMA_EXPECTED_JA3_SAFARI_17`
- `PNEUMA_EXPECTED_JA3_EDGE_120`
