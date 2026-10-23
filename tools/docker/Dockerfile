# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The SIDOU build image: the pinned JieLi pi32v2 toolchain for the firmware, and everything the
# host workspace and the release CI need (a pinned Rust, GCC 13, clang and libclang for
# bindgen). Published as ghcr.io/dsablic/sidou-build by .github/workflows/build-image.yml.
FROM ubuntu:24.04
LABEL org.opencontainers.image.title="sidou-build" \
      org.opencontainers.image.description="Build image for SIDOU, the FM-1 firmware: JieLi pi32v2 toolchain, CMake, Ninja, Rust, GCC 13, clang" \
      org.opencontainers.image.source="https://github.com/dsablic/sidou" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential cmake ninja-build clang libclang-dev pkg-config \
      git jq curl xz-utils unzip ca-certificates \
 && rm -rf /var/lib/apt/lists/*
# The Rust the host workspace builds with (rust-version is 1.88; CI builds with this one).
ARG RUST_VERSION=1.99.0
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL --retry 3 https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" \
 && chmod -R a+rwX /usr/local/rustup /usr/local/cargo
ARG TOOLCHAIN_SHA256
RUN test -n "$TOOLCHAIN_SHA256" \
 && curl -fL --retry 3 -o /tmp/tc.tar.xz https://pkgman.jieliapp.com/s/linux-toolchain \
 && echo "$TOOLCHAIN_SHA256  /tmp/tc.tar.xz" | sha256sum -c - \
 && mkdir -p /opt/jieli \
 && tar -xJf /tmp/tc.tar.xz -C /opt/jieli \
 && ln -s /opt/jieli/jieli-linux-toolchains-* /opt/jieli/toolchain \
 && rm /tmp/tc.tar.xz
ENV JIELI_TOOLCHAIN=/opt/jieli/toolchain
# tools/fm1-docker sees this and runs its command directly instead of starting Docker.
ENV SIDOU_BUILD_IMAGE=1
# CI checks the repository out as another user; git would refuse it.
RUN git config --system --add safe.directory '*'
