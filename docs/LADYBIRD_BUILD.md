# Ladybird Build Setup

## Overview

`pneuma-ladybird-shim` is feature-gated behind `--features ladybird`.
Default workspace builds do not configure or build Ladybird.

## Host Dependencies (Ubuntu/Debian)

```bash
sudo apt-get install -y \
  cmake \
  ninja-build \
  clang-20 \
  nasm \
  autoconf \
  autoconf-archive \
  automake \
  libtool \
  qt6-base-dev \
  qt6-tools-dev \
  qt6-tools-dev-tools \
  libxkbcommon-dev \
  libpulse-dev \
  pkg-config \
  curl \
  zip \
  unzip \
  tar
```

## Pinned Commits

- Ladybird submodule:
  `e87f889e31afbb5fa32c910603c7f5e781c97afd`
- vcpkg bootstrap baseline:
  `2fa7118fb2ce0c27ab73e08ab1991f4cb67af880`

## Build Directory

- Default: `vendor/ladybird/Build/debug-clang20`
- Override: set `LADYBIRD_BUILD_DIR`

```bash
LADYBIRD_BUILD_DIR=/path/to/build \
cargo test -p pneuma-ladybird-shim --features ladybird
```

## Expected Timings

- Cold configure (vcpkg install path): long (hours are possible)
- Warm configure (cache hit): fast (seconds)

## Concurrency Rule

Run only one Ladybird configure at a time. Concurrent configure/build commands
can block on vcpkg filesystem locks.
