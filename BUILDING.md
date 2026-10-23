<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Building SIDOU

SIDOU has two builds:
- **Host** (`host/`): a cargo workspace that compiles the C core for your
  machine and holds every test, benchmark and tool.
- **Firmware** (`fm1` CMake preset): cross-compiles the C core for the FM-1's
  pi32v2 CPU with JieLi's toolchain, inside a pinned Docker image.

The firmware build produces the app image and a bare-metal benchmark
program. The image is not installable yet: packaging it as an update comes
later in M1.

## Get the source

libresidfp, the reference for the host tests, is a git submodule:

```sh
git clone --recurse-submodules https://github.com/dsablic/sidou.git
# or, in an existing clone:
git submodule update --init
```

## Host build

**Prerequisites:**
- Rust 1.88 or newer (the workspace's `rust-version`), for example through
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
C build step. The same goes for the song player's reference (`crates/gt2-ref`), which
builds GoatTracker 2.77's packer from `third_party/goattracker2`: nothing new to install.
Only its `--ignored` assembler cross-check needs `curl` and `unzip`, to fetch GoatTracker's
release into `build/` ([TESTING.md](TESTING.md), "Song player reference"). [TESTING.md](TESTING.md) covers the test layers and the
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

Build a package from a **fresh build directory**: CMake keeps a cached option over a changed
default, so an old `build/fm1` can build a different firmware. Configure warns when an option
is off its default, the image records its options (`SIDOU-OPTIONS` in `sidou_app.bin`), and
`sidou-pack --verify` and `tools/preflash` fail a package whose options are not the defaults
(the flash spike's and the checklist builds' packages fail them by design).

`tools/fm1-docker` runs the command in the build image (below), with the repository mounted
at the same path and your user id. The output goes to `build/fm1/`:
- `platform/fm1/sidou_app.{elf,bin,dis}`: the firmware. The `.bin` is what
  the FM-1's SPL maps at XIP address `0x02000120`.
- `platform/fm1/loader/loader.{elf,bin,dis}`: the update loader, Felucca's,
  unmodified and built as Felucca builds it: a RAM image for `0x01C0A800`.
- `bench/fm1/sid_bench_{1,2}.{elf,bin,dis}`: the benchmark programs, for one
  and two chips;
- `bench/fm1/sid_bench_fx_{1,2}` and `sid_bench_fx_worst_{1,2}`: the same with the
  post-chip effects at full (settled, and in their worst case);
- `bench/fm1/sid_bench_slots_{1,2}`: the effect-slot scenarios.

**Package** (an FM-1 update package, `.fwsc`, that community installers
take), from `host/`:

```sh
cargo run --release -p sidou-pack -- \
    --app ../build/fm1/platform/fm1/sidou_app.bin \
    --loader ../build/fm1/platform/fm1/loader/loader.bin \
    -o ../build/fm1/sidou.fwsc
```

On first use it downloads the inputs that are not SIDOU's into `build/`,
checked against the SHA-256s in `host/crates/sidou-pack/inputs.toml` (`curl`
must be installed); `--offline` uses only what is there:
- `build/ac79/`: three files of JieLi's AC79 SDK (Apache-2.0), from gitee:
  the SPL for the package's head, built from SDK defaults as Felucca's
  packages are, and two files for the app area.

The update loader never writes the flash head, so a device keeps its own;
no byte of M-VAVE's firmware goes into a package.

**Checklist builds** (only for [docs/hardware-test.md](docs/hardware-test.md),
never to be handed out): a firmware that, when OCT- is held at power-on,
dies one second after boot, by a hang the watchdog resets (`hang`) or a P33
reboot (`reboot`), to check the boot guard on a real unit. Without the key
it runs as the normal build:

```sh
tools/fm1-docker cmake --preset fm1 -B build/fm1-death -DSIDOU_TEST_DEATH=hang
tools/fm1-docker cmake --build build/fm1-death
```

Package its `platform/fm1/sidou_app.bin` with the normal loader.

**The flash timing spike** (M2a Task 3b, only for
[docs/hardware-test.md](docs/hardware-test.md) section 12, never to be handed
out): a test firmware that, instead of SIDOU, measures the flash part's erase
and program times on the store's last two sectors and shows them on the
screen. The option is off by default, and the default image has none of it:

```sh
tools/fm1-docker cmake --preset fm1 -B build/fm1-spike -DSIDOU_FLASH_SPIKE=ON
tools/fm1-docker cmake --build build/fm1-spike
```

From `host/`, package it as SIDOU is packaged, with its own app and loader:

```sh
cargo run --release -p sidou-pack -- \
    --app ../build/fm1-spike/platform/fm1/sidou_app.bin \
    --loader ../build/fm1-spike/platform/fm1/loader/loader.bin \
    -o ../build/fm1-spike/sidou-spike.fwsc
```

`sidou-pack --test-wrong-size` makes a package that SIDOU's update step 1
refuses (for the checklist's refused-install item).
`sidou-pack --verify sidou.fwsc` checks a finished package (SHA-256, identity, sizes, CRCs, the
loader's marker, and which loader it is: Felucca 1.6.1's by its SHA-256, with its identity)
without any input or device; `tools/preflash sidou.fwsc` runs it and checks the recovery route
too (`docs/flash-day.md`).

Ship `LICENSES/Apache-2.0.txt` with every package. The package's identity is
`FM-1_900` (`--product`; it must be the firmware's `SIDOU_PRODUCT`).

The toolchain file is `cmake/pi32v2.cmake`. It passes `-mcpu=r3 -mfprev1`,
which turns on the hardware single-precision FPU. Without `-mcpu=r3`, clang
compiles every float operation to a soft-float library call.

To run the firmware benchmark in the emulator, see "Emulator gate" in
[TESTING.md](TESTING.md).

## The build image

`ghcr.io/dsablic/sidou-build` (public, `linux/amd64`) holds everything a build needs: JieLi's
pi32v2 toolchain (pinned by SHA-256 in `tools/docker/toolchain.sha256`), CMake, Ninja, GCC 13,
clang and libclang (bindgen), a pinned Rust (`RUST_VERSION` in `tools/docker/Dockerfile`), git
and jq. The release CI (`.github/workflows/ci.yml`) runs in it, pinned by digest, so a run
installs nothing. Inside it `SIDOU_BUILD_IMAGE` is set, and `tools/fm1-docker` then runs its
command directly instead of starting Docker.

**Locally**, `tools/fm1-docker` pulls the image once: the digest in `tools/docker/image.digest`
if that file has one, else `:latest`. If it cannot (offline, or not published yet), or when
`SIDOU_BUILD_LOCAL=1`, it builds `tools/docker/Dockerfile` itself, tagged `sidou-build:local`
(the first build takes a few minutes; Docker caches it):

```sh
docker build --platform linux/amd64 -t sidou-build:local \
    --build-arg TOOLCHAIN_SHA256="$(cat tools/docker/toolchain.sha256)" tools/docker
```

**Publishing.** `.github/workflows/build-image.yml` builds and pushes `:latest` and `:<commit>`
with the repository's `GITHUB_TOKEN` when `tools/docker/**` changes on `main`, or when run by
hand (Actions, Run workflow). Its summary prints the digest. Then:
1. put the digest (`sha256:...`) in `tools/docker/image.digest`, and in `ci.yml`'s `container.image`
   (`...sidou-build@sha256:...`; it reads `IMAGE_DIGEST_PLACEHOLDER` until the first publish);
2. the first time only: make the package public (GitHub, the package's settings, Change
   visibility), so CI and everyone else can pull it without a login.

To use the image's tools on the host workspace without installing anything, mount a separate
target directory, as the Linux build must not share `host/target` with macOS:

```sh
tools/fm1-docker env HOME=/tmp CARGO_TARGET_DIR=$PWD/build/linux-target \
    sh -c 'cd host && cargo test --locked --release --workspace'
```

`tools/gcc-check` runs GCC 13 over the host tests' C files in the same image
([TESTING.md](TESTING.md), "Continuous integration").

## Browser demo (`tools/web-demo`)

One command builds the working tree's firmware into
[fm1-emu](https://github.com/joakimeriksson/fm1-emu)'s web page and serves it:

```sh
tools/web-demo                  # build, then serve http://localhost:8642/ (Ctrl-C stops)
tools/web-demo --port 9000      # another port
tools/web-demo --skip-firmware  # reuse build/fm1's firmware (no Docker); the rest is redone
tools/web-demo --no-serve       # build only
tools/web-demo --check          # build, then the headless smoke test (TESTING.md)
```

**Prerequisites:** those of the firmware build and the host build, Python 3,
and git. `--check` also needs Node.js 20 or later. The wasm needs Rust's
`wasm32-unknown-unknown` target; with rustup the script adds it.

**What it does:**
1. builds the `fm1` firmware in Docker and packages it (`sidou-pack`), as
   above, into `build/fm1/sidou.fwsc`;
2. fetches fm1-emu into `build/fm1-emu` at the revision
   `host/crates/emu-gate/Cargo.toml` pins (one pin for the emulator tests
   and the demo: bump it there) and runs fm1-emu's own `web/build.sh`,
   which builds the `fm1-web` crate to wasm and copies the page's files
   (`main.js`, `persistence.js`, `worklet.js`, `jit.js`, `fwsc.js`) into
   `build/web/`;
3. puts SIDOU's layer from `web/` on top: `index.html` (the SIDOU page, in
   place of upstream's), `sidou.js`, `firmwares.json` (a catalog of SIDOU
   alone, the package served next to the page), the logo, the licence
   texts the package and the emulator need, and `version.json` (the commit,
   its date, whether the tree had local changes);
4. patches upstream's `main.js` with `web/patch-main.sh` (two lines: a
   default clock of 240 MHz, and Start enabled once its handler is attached;
   each edit must apply exactly once, so a new upstream revision that moves
   them stops the build);
5. serves `build/web/` on 127.0.0.1 with `web/serve.py` (no caching, so a
   rebuild shows on reload).

Upstream's files are copied at build time, never committed: they stay at
the pinned revision and under fm1-emu's MIT licence. SIDOU's own files for
the page live in `web/` (`platform/web` in the design document is kept for
a future native wasm build of the core). `.github/workflows/pages.yml`
runs `tools/web-demo --check` and deploys `build/web/` to GitHub Pages.

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
