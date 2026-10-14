# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake ninja-build curl xz-utils ca-certificates \
 && rm -rf /var/lib/apt/lists/*
ARG TOOLCHAIN_SHA256
RUN test -n "$TOOLCHAIN_SHA256" \
 && curl -fL --retry 3 -o /tmp/tc.tar.xz https://pkgman.jieliapp.com/s/linux-toolchain \
 && echo "$TOOLCHAIN_SHA256  /tmp/tc.tar.xz" | sha256sum -c - \
 && mkdir -p /opt/jieli \
 && tar -xJf /tmp/tc.tar.xz -C /opt/jieli \
 && ln -s /opt/jieli/jieli-linux-toolchains-* /opt/jieli/toolchain \
 && rm /tmp/tc.tar.xz
ENV JIELI_TOOLCHAIN=/opt/jieli/toolchain
