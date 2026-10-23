# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The SIDOU build image: the pinned JieLi pi32v2 toolchain for the firmware, and everything the
# host workspace and the release CI need (a pinned Rust, GCC 13, clang and libclang for
# bindgen). Published as ghcr.io/dsablic/sidou-build by .github/workflows/build-image.yml.
FROM ubuntu:24.04
LABEL org.opencontainers.image.title="sidou-build" \
      org.opencontainers.image.description="Build image for SIDOU, the FM-1 firmware: JieLi pi32v2 toolchain, CMake, Ninja, Rust (+wasm32), GCC 13, clang, Node and Playwright's Chromium for the browser demo" \
      org.opencontainers.image.source="https://github.com/dsablic/sidou" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential cmake ninja-build clang libclang-dev pkg-config \
      git jq curl xz-utils unzip ca-certificates python3 \
 && rm -rf /var/lib/apt/lists/*
# The Rust the host workspace builds with (rust-version is 1.88; CI builds with this one).
ARG RUST_VERSION=1.99.0
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL --retry 3 https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" \
      --target wasm32-unknown-unknown \
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
# The browser demo (pages.yml, tools/web-demo --check): Node, and the Playwright version
# web/check.sh pins with its Chromium and the system libraries it needs, so no job installs
# anything with apt (an apt step hung pages.yml and ci.yml's tag runs, 2026-10-23).
ARG NODE_VERSION=22.23.4
ARG NODE_SHA256=1805bcd35e6815559bd2eb97db438d3fdb0eb3a5e6a977b7c1badca76337e9ce
RUN curl -fL --retry 3 -o /tmp/node.tar.xz "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.xz" \
 && echo "$NODE_SHA256  /tmp/node.tar.xz" | sha256sum -c - \
 && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 \
 && rm /tmp/node.tar.xz
ARG PLAYWRIGHT_VERSION=1.63.0
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
RUN npx -y "playwright@$PLAYWRIGHT_VERSION" install --with-deps --only-shell chromium \
 && rm -rf /var/lib/apt/lists/* /root/.npm \
 && chmod -R a+rX "$PLAYWRIGHT_BROWSERS_PATH"
ENV SIDOU_BUILD_IMAGE=1
# CI checks the repository out as another user; git would refuse it.
RUN git config --system --add safe.directory '*'
