<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Building SIDOU

SIDOU has two builds:
- **Host** (`host/`): a cargo workspace that compiles the C core for your
  machine and holds every test, benchmark and tool.
- **Firmware** (`fm1` CMake preset): cross-compiles the C core for the FM-1's
  pi32v2 CPU with JieLi's toolchain, inside a pinned Docker image.

In M0 the firmware build produces a bare-metal benchmark program, not an
installable image.

## Get the source

libresidfp, the reference for the host tests, is a git submodule:

```sh
git clone --recurse-submodules https://github.com/dsablic/sidou.git
# or, in an existing clone:
git submodule update --init
```

## Host build

**Prerequisites:**
- Rust 1.87 or newer (the workspace's `rust-version`), for example through
  rustup;
- a C11 and C++17 compiler (the C++ is reSIDfp, test-only);
- libclang, for bindgen. macOS has it with the Xcode command line tools; on
  Debian or Ubuntu, install `libclang-dev` and `clang`.

**Build and test**, from `host/`:

```sh
cd host
cargo build --workspace
cargo test --workspace --release
```

`cargo test` compiles the C core with the `cc` crate, so there is no separate
C build step. [TESTING.md](TESTING.md) covers the test layers and the
sanitizer runs.

**Compiler flags from the environment.** `cc` adds `CFLAGS` and `CXXFLAGS`
to the C and C++ compiles. Host and pi32v2 float results must be
bit-identical, so leave out any flag that changes floating-point behaviour,
such as `-ffast-math`, `-ffp-contract=fast` or `-ffinite-math-only`. They
change the golden hashes and break the host/target comparison.
`-ffast-math` stops the build with an `#error` from `core/sid/sid_math.h`;
the others are not caught.

## Firmware build (`fm1` preset)

**Prerequisites:**
- Docker. The image is `linux/amd64`; on Apple Silicon, Docker Desktop runs
  it under emulation.
- Nothing else. CMake, Ninja and the JieLi toolchain all run inside the
  image.

**Build**, from the repository root:

```sh
tools/fm1-docker cmake --preset fm1
tools/fm1-docker cmake --build --preset fm1
```

`tools/fm1-docker` builds the image (`sidou-pi32v2`) if needed and runs the
command in it, with the repository mounted at the same path. The output goes
to `build/fm1/`. The benchmark programs are
`build/fm1/bench/fm1/sid_bench_{1,2}.{elf,bin,dis}`, for one and two chips.

The toolchain file is `cmake/pi32v2.cmake`. It passes `-mcpu=r3 -mfprev1`,
which turns on the hardware single-precision FPU. Without `-mcpu=r3`, clang
compiles every float operation to a soft-float library call.

To run the firmware benchmark in the emulator, see "Emulator gate" in
[TESTING.md](TESTING.md).

## The pinned toolchain

The image downloads JieLi's Linux toolchain
(`jieli-linux-toolchains-20260730.1`, clang 4.0.1) from
`https://pkgman.jieliapp.com/s/linux-toolchain`. That URL is not versioned,
so the image checks the archive against the SHA-256 in
`tools/docker/toolchain.sha256`, and the image build fails if they differ.

**When the hash check fails,** JieLi has most likely published a new
release at the same URL. Don't just copy the new hash in. Instead:

1. Download the archive and compute its hash:

   ```sh
   curl -fLo tc.tar.xz https://pkgman.jieliapp.com/s/linux-toolchain
   shasum -a 256 tc.tar.xz
   ```

2. Check that it is a genuine JieLi release: it came from JieLi's own
   server over HTTPS, and its top-level directory is named
   `jieli-linux-toolchains-<version>` (`tar -tJf tc.tar.xz | head -1`).
3. Put the new hash in `tools/docker/toolchain.sha256`, then check the
   compiler in the rebuilt image:

   ```sh
   tools/fm1-docker sh -c '"$JIELI_TOOLCHAIN/pi32v2/bin/clang" --version'
   ```

   Commit the hash on its own, naming the new release and compiler version.
4. Rebuild the `fm1` preset and run the emulator gate. Compare the
   instructions per sample with
   [decision record 0002](docs/decisions/0002-m0-sid-cost.md). A different
   compiler can change the count; explain any change in that commit.
