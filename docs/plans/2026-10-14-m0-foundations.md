# M0 Foundations Implementation Plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the first SIDOU SID core and the test and benchmark harness
around it. Then answer M0's question: does one SID, including the 6581
filter, cost no more than 900 pi32v2 instructions per sample on the FM-1?

**Architecture:** The SID core is plain C11 in `core/sid/`, with no platform
code and no libc beyond `<stdint.h>` and `<string.h>`. It builds three ways:
- for the host, under GoogleTest and Google Benchmark, using CMake and CTest;
- for the pi32v2 chip, with JieLi's clang inside a pinned Docker image, as a
  bare-metal benchmark program;
- for a Rust test crate, which runs the pi32v2 benchmark in fm1-emu (used as a
  library) and compares its output with the host build of the same C code.

reSIDfp is fetched for the host tests only, as the reference to compare
against.

**Tech stack:**
- C11 (core);
- C++17 (tests, reference harness);
- CMake ≥ 3.25 with presets, Ninja, CTest;
- GoogleTest v1.15.2, Google Benchmark v1.9.1;
- libresidfp at commit `c1791383ca843a855e5d4366da6a7277e34e801d`;
- JieLi pi32v2 clang 4.0.1 in Docker (`linux/amd64`);
- Rust stable, with fm1-emu crates at commit
  `e52f4640157f490f8c3af778dd044e9d33e989e8`;
- GitHub Actions; REUSE.

**Pre-check status (2026-10-14):**
- **Run and passing on macOS arm64:** the C code and the host tests of
  Tasks 1–8 and 10, built from this document's code blocks in a scratch
  directory. That covers 37 unit and golden tests, plus 4 reSIDfp reference
  tests, under both RelWithDebInfo and ASan+UBSan.
- **Not run yet:** Tasks 9 and 11–14 (Google Benchmark, Docker toolchain,
  pi32v2 build, emulator gate, CI). Expect to adjust details there.

**Context to read first:**
- `docs/plans/2026-10-14-sidou-design.md`, sections 2, 4, 8, 9, 10, 11
  and 15.
- Reference implementations (behaviour, not code to copy):
  - reSID 0.16: `envelope.cc`, `wave.cc`, `filter.cc`;
  - fm1-emu's `fm1-sid/README.md`, which has the measurements and the
    bare-metal benchmark method;
  - Cytomic's "SvfLinearTrapOptimised2" paper (Andrew Simper), which defines
    the filter used here.

**Conventions:**
- **Includes.** The include root is `core/`, so code outside `core/sid/`
  writes `#include "sid/sid.h"`. Never add `core/sid` to the include path. On
  case-insensitive filesystems (macOS), `sid.h` and reSIDfp's `SID.h`
  collide; the pre-check of this plan hit exactly that.
- **Licence headers.** Every new file starts with
  `SPDX-FileCopyrightText: 2026 SIDOU contributors` and
  `SPDX-License-Identifier: GPL-3.0-or-later`, unless the task says
  otherwise. Files that can't hold comments go in `REUSE.toml`.
- **Code style.** Comments only where the *why* isn't obvious. Names are
  `snake_case`, and C types carry a `sid_` / `sidou_` prefix.
- **Commits.** Commit after every task, with Conventional-Commit style
  subjects. End each message with the
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` trailer.

**Out of scope for M0:**
- combined-waveform tables (M0 uses AND as a stand-in; same cost class);
- chip profiles and the character layer;
- the second core;
- any UI or HAL;
- `.fwsc` packaging.

---

### Task 1: Build skeleton, presets and the first test

**Files:**
- Create: `.gitignore`, `.editorconfig`, `.clang-format`
- Create: `CMakeLists.txt`, `CMakePresets.json`
- Create: `core/CMakeLists.txt`, `core/sid/sid_defs.h`
- Create: `tests/CMakeLists.txt`, `tests/unit/sid_defs_test.cpp`
- Modify: `REUSE.toml`

**Step 1: Write the repo hygiene files**

`.gitignore`:
```gitignore
/build/
/tests/target/target/
*.wav
.DS_Store
```

`.editorconfig`:
```ini
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
indent_style = space
indent_size = 4

[*.{yml,yaml,json,md}]
indent_size = 2
```

`.clang-format`:
```yaml
BasedOnStyle: LLVM
IndentWidth: 4
ColumnLimit: 100
PointerAlignment: Left
AllowShortFunctionsOnASingleLine: Inline
```

Add SPDX headers as comments (`#` for the first two files; `.clang-format` is
YAML, so use `#` there too).

**Step 2: Write the top-level CMake and presets**

`CMakeLists.txt`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required(VERSION 3.25)
project(sidou VERSION 0.0.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

add_subdirectory(core)

if(NOT CMAKE_CROSSCOMPILING)
    enable_language(CXX)
    set(CMAKE_CXX_STANDARD 17)
    set(CMAKE_CXX_STANDARD_REQUIRED ON)
    enable_testing()
    add_subdirectory(tests)
endif()
```

`CMakePresets.json`:
```json
{
  "version": 6,
  "configurePresets": [
    {
      "name": "base",
      "hidden": true,
      "generator": "Ninja",
      "binaryDir": "${sourceDir}/build/${presetName}"
    },
    {
      "name": "host",
      "inherits": "base",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "RelWithDebInfo" }
    },
    {
      "name": "host-asan",
      "inherits": "base",
      "cacheVariables": {
        "CMAKE_BUILD_TYPE": "Debug",
        "CMAKE_C_FLAGS": "-fsanitize=address,undefined -fno-omit-frame-pointer -fno-sanitize-recover=all",
        "CMAKE_CXX_FLAGS": "-fsanitize=address,undefined -fno-omit-frame-pointer -fno-sanitize-recover=all",
        "CMAKE_EXE_LINKER_FLAGS": "-fsanitize=address,undefined"
      }
    },
    {
      "name": "fm1",
      "inherits": "base",
      "toolchainFile": "${sourceDir}/cmake/pi32v2.cmake",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" }
    }
  ],
  "buildPresets": [
    { "name": "host", "configurePreset": "host" },
    { "name": "host-asan", "configurePreset": "host-asan" },
    { "name": "fm1", "configurePreset": "fm1" }
  ],
  "testPresets": [
    { "name": "host", "configurePreset": "host", "output": { "outputOnFailure": true } },
    { "name": "host-asan", "configurePreset": "host-asan", "output": { "outputOnFailure": true } }
  ]
}
```

**Step 3: Core library with the shared definitions**

`core/sid/sid_defs.h`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_DEFS_H
#define SIDOU_SID_DEFS_H

#define SID_CLOCK_PAL 985248u
#define SID_CLOCK_NTSC 1022727u

typedef enum { SID_MODEL_6581 = 0, SID_MODEL_8580 = 1 } sid_model;

#endif
```

`core/CMakeLists.txt` (the library gets its sources in Task 6. Until then it
is an INTERFACE library):
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_core INTERFACE)
target_include_directories(sidou_core INTERFACE ${CMAKE_CURRENT_SOURCE_DIR})
```

**Step 4: GoogleTest wiring and a first test**

`tests/CMakeLists.txt`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
include(FetchContent)
FetchContent_Declare(googletest
    GIT_REPOSITORY https://github.com/google/googletest.git
    GIT_TAG v1.15.2
    GIT_SHALLOW TRUE)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googletest)
include(GoogleTest)

add_executable(sidou_unit_tests
    unit/sid_defs_test.cpp)
target_link_libraries(sidou_unit_tests PRIVATE sidou_core GTest::gtest_main)
target_compile_options(sidou_unit_tests PRIVATE -Wall -Wextra)
gtest_discover_tests(sidou_unit_tests)
```

`tests/unit/sid_defs_test.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_defs.h"
}

TEST(SidDefs, PalClockIsTheC64Value) {
    EXPECT_EQ(SID_CLOCK_PAL, 985248u);
}
```

**Step 5: Add `CMakePresets.json` to `REUSE.toml`**

Append to `REUSE.toml`:
```toml
[[annotations]]
path = ["CMakePresets.json"]
SPDX-FileCopyrightText = "2026 SIDOU contributors"
SPDX-License-Identifier = "GPL-3.0-or-later"
```

**Step 6: Configure, build, test**

Run: `cmake --preset host && cmake --build --preset host && ctest --preset host`
Expected: `100% tests passed, 0 tests failed out of 1`

Run: `reuse lint`
Expected: `Congratulations! Your project is compliant`

**Step 7: Commit**

```bash
git add -A
git commit -m "build: add CMake presets, core library and GoogleTest harness"
```

---

### Task 2: Oscillator: accumulator, saw, triangle and pulse

The SID oscillator is a 24-bit phase accumulator. The core advances it by a
whole number of chip cycles per output sample (22 or 23 at PAL/44.1 kHz), so
pitch stays exactly right.

**Files:**
- Create: `core/sid/sid_osc.h`
- Create: `tests/unit/sid_osc_test.cpp`
- Modify: `tests/CMakeLists.txt` (add the test source)

**Step 1: Write the failing tests**

`tests/unit/sid_osc_test.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

namespace {

sid_osc make_osc(uint8_t control, uint32_t freq) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, control);
    o.freq = freq;
    return o;
}

} // namespace

TEST(SidOsc, AccumulatorAddsFrequencyEveryCycle) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x1000);
    sid_osc_advance(&o, 3);
    EXPECT_EQ(o.acc, 0x3000u);
}

TEST(SidOsc, AccumulatorWrapsAt24BitsAndReportsMsbRise) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x100000);
    sid_osc_advance(&o, 16);
    EXPECT_EQ(o.acc, 0u);
    EXPECT_EQ(o.msb_rose, 1);
    EXPECT_EQ(o.overshoot, 0x800000u); // crossed 0x800000 eight cycles ago
}

TEST(SidOsc, SawIsTheTop12Bits) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x1000);
    sid_osc_advance(&o, 3);
    EXPECT_EQ(sid_osc_output(&o, &o), 3u);
}

TEST(SidOsc, TriangleFoldsOnTheMsb) {
    sid_osc o = make_osc(SID_CTRL_TRI, 0);
    o.acc = 0x400000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x800u);
    o.acc = 0x7fffff;
    EXPECT_EQ(sid_osc_output(&o, &o), 0xfffu);
    o.acc = 0xc00000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x7ffu);
}

TEST(SidOsc, RingModXorsTheSourceMsbIntoTheTriangle) {
    sid_osc o = make_osc(SID_CTRL_TRI | SID_CTRL_RING, 0);
    sid_osc src = make_osc(SID_CTRL_SAW, 0);
    o.acc = 0x400000;
    src.acc = 0x800000;
    EXPECT_EQ(sid_osc_output(&o, &src), 0x7ffu);
}

TEST(SidOsc, PulseComparesTop12BitsWithPulseWidth) {
    sid_osc o = make_osc(SID_CTRL_PULSE, 0);
    o.pw = 0x800;
    o.acc = 0x7ff000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x000u);
    o.acc = 0x800000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0xfffu);
}

TEST(SidOsc, NoWaveformSelectedOutputsZero) {
    sid_osc o = make_osc(0, 0x1000);
    sid_osc_advance(&o, 100);
    EXPECT_EQ(sid_osc_output(&o, &o), 0u);
}

TEST(SidOsc, TestBitResetsAndHoldsTheAccumulator) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x1000);
    sid_osc_advance(&o, 10);
    sid_osc_write_control(&o, SID_CTRL_SAW | SID_CTRL_TEST);
    sid_osc_advance(&o, 10);
    EXPECT_EQ(o.acc, 0u);
}

TEST(SidOsc, CombinedWaveformsAreAndedForNow) {
    sid_osc o = make_osc(SID_CTRL_SAW | SID_CTRL_PULSE, 0);
    o.pw = 0x100;
    o.acc = 0x5a5000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x5a5u & 0xfffu);
}
```

Add `unit/sid_osc_test.cpp` to `sidou_unit_tests` in `tests/CMakeLists.txt`.

**Step 2: Run them and watch them fail**

Run: `cmake --build --preset host`
Expected: a compile error, `sid_osc.h: No such file or directory`

**Step 3: Implement `core/sid/sid_osc.h`**

```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_OSC_H
#define SIDOU_SID_OSC_H

#include <stdint.h>

#define SID_CTRL_GATE 0x01u
#define SID_CTRL_SYNC 0x02u
#define SID_CTRL_RING 0x04u
#define SID_CTRL_TEST 0x08u
#define SID_CTRL_TRI 0x10u
#define SID_CTRL_SAW 0x20u
#define SID_CTRL_PULSE 0x40u
#define SID_CTRL_NOISE 0x80u

#define SID_LFSR_RESET 0x7ffff8u

typedef struct {
    uint32_t acc;
    uint32_t freq;
    uint32_t pw;
    uint32_t lfsr;
    uint32_t overshoot; /* accumulator units past the last MSB rise, for hard sync */
    uint8_t control;
    uint8_t msb_rose;
} sid_osc;

static inline void sid_osc_reset(sid_osc* o) {
    o->acc = 0;
    o->freq = 0;
    o->pw = 0;
    o->lfsr = SID_LFSR_RESET;
    o->overshoot = 0;
    o->control = 0;
    o->msb_rose = 0;
}

static inline void sid_lfsr_clock(sid_osc* o) {
    uint32_t bit = ((o->lfsr >> 22) ^ (o->lfsr >> 17)) & 1u;
    o->lfsr = ((o->lfsr << 1) | bit) & 0x7fffffu;
}

static inline uint32_t sid_noise_output(uint32_t r) {
    return ((r & 0x400000u) >> 11) | ((r & 0x100000u) >> 10) | ((r & 0x010000u) >> 7) |
           ((r & 0x002000u) >> 5) | ((r & 0x000800u) >> 4) | ((r & 0x000080u) >> 1) |
           ((r & 0x000010u) << 1) | ((r & 0x000004u) << 2);
}

static inline void sid_osc_write_control(sid_osc* o, uint8_t v) {
    if ((v & SID_CTRL_TEST) && !(o->control & SID_CTRL_TEST)) {
        o->acc = 0;
        o->lfsr = SID_LFSR_RESET;
    }
    o->control = v;
}

/* cycles is at most ~23, so freq * cycles < 2^24 and at most one MSB rise can occur. */
static inline void sid_osc_advance(sid_osc* o, uint32_t cycles) {
    o->msb_rose = 0;
    if (o->control & SID_CTRL_TEST) {
        return;
    }
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles;
    uint32_t noise_clocks = ((next + 0x80000u) >> 20) - ((prev + 0x80000u) >> 20);
    while (noise_clocks--) {
        sid_lfsr_clock(o);
    }
    if (((next + 0x800000u) >> 24) != ((prev + 0x800000u) >> 24)) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
}

/* Hard sync: restart o as if it had been reset at the moment src's MSB rose. */
static inline void sid_osc_sync(sid_osc* o, const sid_osc* src) {
    uint32_t cycles_after = src->overshoot / src->freq;
    o->acc = (o->freq * cycles_after) & 0xffffffu;
}

static inline uint32_t sid_osc_output(const sid_osc* o, const sid_osc* src) {
    uint32_t c = o->control;
    if (!(c & 0xf0u)) {
        return 0;
    }
    uint32_t out = 0xfffu;
    if (c & SID_CTRL_TRI) {
        uint32_t msb = o->acc & 0x800000u;
        if (c & SID_CTRL_RING) {
            msb ^= src->acc & 0x800000u;
        }
        out &= ((msb ? ~o->acc : o->acc) >> 11) & 0xfffu;
    }
    if (c & SID_CTRL_SAW) {
        out &= o->acc >> 12;
    }
    if (c & SID_CTRL_PULSE) {
        out &= ((c & SID_CTRL_TEST) || (o->acc >> 12) >= o->pw) ? 0xfffu : 0u;
    }
    if (c & SID_CTRL_NOISE) {
        out &= sid_noise_output(o->lfsr);
    }
    return out;
}

#endif
```

**Step 4: Run the tests**

Run: `cmake --build --preset host && ctest --preset host -R SidOsc`
Expected: all `SidOsc.*` tests pass.

**Step 5: Commit**

```bash
git add core/sid/sid_osc.h tests/unit/sid_osc_test.cpp tests/CMakeLists.txt
git commit -m "feat(sid): oscillator accumulator, saw, triangle, pulse, ring mod"
```

---

### Task 3: Noise LFSR clocking

**Files:**
- Create: `tests/unit/sid_noise_test.cpp`
- Modify: `tests/CMakeLists.txt`

The implementation is already in `sid_osc.h`. This task adds tests that pin
down its behaviour: the LFSR clocks on a rising edge of accumulator bit 19, is
maximal-length, and uses reSID's output tap mapping.

**Step 1: Write the tests**

```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

TEST(SidNoise, ClocksOnEveryRisingEdgeOfBit19) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, SID_CTRL_NOISE);
    o.freq = 0x80000; // bit 19 toggles every cycle, so it rises every 2 cycles

    sid_osc reference = o;
    for (int i = 0; i < 10; ++i) {
        sid_lfsr_clock(&reference);
    }
    sid_osc_advance(&o, 20);
    EXPECT_EQ(o.lfsr, reference.lfsr);
}

TEST(SidNoise, FirstClockFromResetShiftsInZero) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_lfsr_clock(&o);
    EXPECT_EQ(o.lfsr, 0x7ffff0u);
}

TEST(SidNoise, OutputUsesReSidTapMapping) {
    EXPECT_EQ(sid_noise_output(SID_LFSR_RESET), 0xfe0u);
    EXPECT_EQ(sid_noise_output(0x400000u), 0x800u);
    EXPECT_EQ(sid_noise_output(0x000004u), 0x010u);
}

TEST(SidNoise, LfsrIsMaximalLength) {
    sid_osc o;
    sid_osc_reset(&o);
    for (uint32_t i = 0; i < 0x7fffffu; ++i) {
        sid_lfsr_clock(&o);
    }
    EXPECT_EQ(o.lfsr, SID_LFSR_RESET);
}
```

**Step 2: Run them**

Run: `cmake --build --preset host && ctest --preset host -R SidNoise`
Expected: PASS. If `LfsrIsMaximalLength` fails, the feedback taps are wrong.
Fix `sid_lfsr_clock` and don't touch the test.

**Step 3: Commit**

```bash
git add tests/unit/sid_noise_test.cpp tests/CMakeLists.txt
git commit -m "test(sid): pin noise LFSR clocking, taps and period"
```

---

### Task 4: Hard sync

**Files:**
- Create: `tests/unit/sid_sync_test.cpp`
- Modify: `tests/CMakeLists.txt`

**Step 1: Write the tests**

```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

namespace {

void setup(sid_osc* target, sid_osc* src) {
    sid_osc_reset(target);
    sid_osc_reset(src);
    sid_osc_write_control(target, SID_CTRL_SAW | SID_CTRL_SYNC);
    sid_osc_write_control(src, SID_CTRL_SAW);
    target->freq = 0x1000;
    target->acc = 0x123456;
    src->freq = 0x2000;
    src->acc = 0x7ff000;
}

} // namespace

TEST(SidSync, ResetsTargetWhenSourceMsbRisesOnTheLastCycle) {
    sid_osc target, src;
    setup(&target, &src);
    sid_osc_advance(&src, 1); // 0x7ff000 -> 0x801000: rose this cycle
    ASSERT_EQ(src.msb_rose, 1);
    sid_osc_sync(&target, &src);
    EXPECT_EQ(target.acc, 0u);
}

TEST(SidSync, TargetKeepsCyclesThatElapsedAfterTheRise) {
    sid_osc target, src;
    setup(&target, &src);
    sid_osc_advance(&src, 2); // rose one full cycle before the end of the step
    ASSERT_EQ(src.msb_rose, 1);
    sid_osc_sync(&target, &src);
    EXPECT_EQ(target.acc, 0x1000u);
}

TEST(SidSync, NoRiseNoReset) {
    sid_osc target, src;
    setup(&target, &src);
    src.acc = 0;
    sid_osc_advance(&src, 2);
    EXPECT_EQ(src.msb_rose, 0);
}
```

**Step 2: Run them**

Run: `cmake --build --preset host && ctest --preset host -R SidSync`
Expected: PASS.

**Step 3: Commit**

```bash
git add tests/unit/sid_sync_test.cpp tests/CMakeLists.txt
git commit -m "test(sid): hard sync timing"
```

---

### Task 5: Envelope generator, including the ADSR delay bug

The envelope behaves like reSID 0.16:
- a 15-bit rate counter compared for *equality* with the period, so lowering
  the period below the counter makes it wrap through 0x7fff (the "ADSR bug"
  that trackers rely on);
- an exponential divider during decay and release;
- the counter freezing at zero.

**Files:**
- Create: `core/sid/sid_env.h`
- Create: `tests/unit/sid_env_test.cpp`
- Modify: `tests/CMakeLists.txt`

**Step 1: Write the failing tests**

```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_env.h"
}

namespace {

sid_env gated(uint8_t ad, uint8_t sr) {
    sid_env e;
    sid_env_reset(&e);
    sid_env_write_ad(&e, ad);
    sid_env_write_sr(&e, sr);
    sid_env_set_gate(&e, 1);
    return e;
}

} // namespace

TEST(SidEnv, StartsSilentAndFrozen) {
    sid_env e;
    sid_env_reset(&e);
    sid_env_advance(&e, 100000);
    EXPECT_EQ(e.counter, 0);
}

TEST(SidEnv, FastestAttackStepsEveryNineCycles) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 8);
    EXPECT_EQ(e.counter, 0);
    sid_env_advance(&e, 1);
    EXPECT_EQ(e.counter, 1);
}

TEST(SidEnv, AttackReachesFullScaleThenDecays) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 9 * 255);
    EXPECT_EQ(e.counter, 0xff);
    EXPECT_EQ(e.state, SID_ENV_DECAY_SUSTAIN);
}

TEST(SidEnv, DecayStopsAtSustainLevel) {
    sid_env e = gated(0x00, 0x80);
    sid_env_advance(&e, 9 * 255 + 9 * (0xff - 0x88));
    EXPECT_EQ(e.counter, 0x88);
    sid_env_advance(&e, 100000);
    EXPECT_EQ(e.counter, 0x88);
}

TEST(SidEnv, ExponentialDividerDoublesStepTimeBelow0x5d) {
    sid_env e = gated(0x00, 0x00);
    sid_env_advance(&e, 9 * 255); // finish the attack, which also passes 0x5d
    ASSERT_EQ(e.state, SID_ENV_DECAY_SUSTAIN);
    while (e.counter != 0x5d) {
        sid_env_advance(&e, 1);
    }
    uint32_t cycles = 0;
    while (e.counter == 0x5d) {
        sid_env_advance(&e, 1);
        ++cycles;
    }
    EXPECT_EQ(cycles, 18u);
}

TEST(SidEnv, ReleaseFreezesAtZero) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 9 * 255);
    sid_env_set_gate(&e, 0);
    sid_env_advance(&e, 2000000);
    EXPECT_EQ(e.counter, 0);
}

TEST(SidEnv, LoweringThePeriodBelowTheCounterWrapsThrough0x7fff) {
    sid_env e = gated(0xf0, 0xf0);   // attack 15: period 31251
    sid_env_advance(&e, 1000);
    ASSERT_EQ(e.counter, 0);
    sid_env_write_ad(&e, 0x00);      // period 9, but the counter is already at 1000
    sid_env_advance(&e, 0x7fff - 1000 + 9 - 1);
    EXPECT_EQ(e.counter, 0);
    sid_env_advance(&e, 1);
    EXPECT_EQ(e.counter, 1);
}
```

**Step 2: Run them and watch them fail**

Run: `cmake --build --preset host`
Expected: a compile error, `sid_env.h: No such file or directory`

**Step 3: Implement `core/sid/sid_env.h`**

```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_ENV_H
#define SIDOU_SID_ENV_H

#include <stdint.h>

typedef enum { SID_ENV_ATTACK, SID_ENV_DECAY_SUSTAIN, SID_ENV_RELEASE } sid_env_state;

/* Rate counter periods in chip cycles, as measured for reSID. */
static const uint16_t SID_RATE_PERIOD[16] = {9,   32,  63,   95,   149,  220,   267,   313,
                                             392, 977, 1954, 3126, 3907, 11720, 19532, 31251};

typedef struct {
    uint32_t rate_counter;
    uint32_t rate_period;
    uint32_t exp_counter;
    uint32_t exp_period;
    sid_env_state state;
    uint8_t counter;
    uint8_t attack, decay, sustain, release;
    uint8_t gate;
    uint8_t hold_zero;
} sid_env;

static inline void sid_env_reset(sid_env* e) {
    e->rate_counter = 0;
    e->rate_period = SID_RATE_PERIOD[0];
    e->exp_counter = 0;
    e->exp_period = 1;
    e->state = SID_ENV_RELEASE;
    e->counter = 0;
    e->attack = e->decay = e->sustain = e->release = 0;
    e->gate = 0;
    e->hold_zero = 1;
}

static inline void sid_env_set_gate(sid_env* e, uint8_t gate) {
    if (!e->gate && gate) {
        e->state = SID_ENV_ATTACK;
        e->rate_period = SID_RATE_PERIOD[e->attack];
        e->hold_zero = 0;
    } else if (e->gate && !gate) {
        e->state = SID_ENV_RELEASE;
        e->rate_period = SID_RATE_PERIOD[e->release];
    }
    e->gate = gate;
}

static inline void sid_env_write_ad(sid_env* e, uint8_t v) {
    e->attack = v >> 4;
    e->decay = v & 0x0f;
    if (e->state == SID_ENV_ATTACK) {
        e->rate_period = SID_RATE_PERIOD[e->attack];
    } else if (e->state == SID_ENV_DECAY_SUSTAIN) {
        e->rate_period = SID_RATE_PERIOD[e->decay];
    }
}

static inline void sid_env_write_sr(sid_env* e, uint8_t v) {
    e->sustain = v >> 4;
    e->release = v & 0x0f;
    if (e->state == SID_ENV_RELEASE) {
        e->rate_period = SID_RATE_PERIOD[e->release];
    }
}

static inline void sid_env_tick(sid_env* e) {
    if (e->state != SID_ENV_ATTACK && ++e->exp_counter != e->exp_period) {
        return;
    }
    e->exp_counter = 0;
    if (e->hold_zero) {
        return;
    }
    switch (e->state) {
    case SID_ENV_ATTACK:
        e->counter = (uint8_t)(e->counter + 1);
        if (e->counter == 0xff) {
            e->state = SID_ENV_DECAY_SUSTAIN;
            e->rate_period = SID_RATE_PERIOD[e->decay];
        }
        break;
    case SID_ENV_DECAY_SUSTAIN:
        if (e->counter != (uint8_t)(e->sustain * 0x11)) {
            --e->counter;
        }
        break;
    case SID_ENV_RELEASE:
        e->counter = (uint8_t)(e->counter - 1);
        break;
    }
    switch (e->counter) {
    case 0xff: e->exp_period = 1; break;
    case 0x5d: e->exp_period = 2; break;
    case 0x36: e->exp_period = 4; break;
    case 0x1a: e->exp_period = 8; break;
    case 0x0e: e->exp_period = 16; break;
    case 0x06: e->exp_period = 30; break;
    case 0x00:
        e->exp_period = 1;
        e->hold_zero = 1;
        break;
    default: break;
    }
}

/* The 15-bit rate counter goes 0x7fff -> 1, so a counter already past the period
 * needs (0x7fff - counter + period) cycles to match it again. */
static inline void sid_env_advance(sid_env* e, uint32_t cycles) {
    while (cycles) {
        uint32_t to_hit = e->rate_counter < e->rate_period
                              ? e->rate_period - e->rate_counter
                              : 0x7fffu - e->rate_counter + e->rate_period;
        if (cycles < to_hit) {
            uint32_t c = e->rate_counter + cycles;
            e->rate_counter = c >= 0x8000u ? c - 0x7fffu : c;
            return;
        }
        cycles -= to_hit;
        e->rate_counter = 0;
        sid_env_tick(e);
    }
}

#endif
```

**Step 4: Run the tests**

Run: `cmake --build --preset host && ctest --preset host -R SidEnv`
Expected: all `SidEnv.*` tests pass.

**Step 5: Commit**

```bash
git add core/sid/sid_env.h tests/unit/sid_env_test.cpp tests/CMakeLists.txt
git commit -m "feat(sid): envelope generator with exponential decay and ADSR delay bug"
```

---

### Task 6: Filter: zero-delay-feedback SVF and cutoff curves

The filter is a Cytomic trapezoidal state-variable filter, which stays stable
up to Nyquist at 44.1 kHz. The cutoff curves are reSID 0.16's measured
points, interpolated linearly. They are **stand-ins**: M1 replaces them with
curves derived from reSIDfp.

The 6581's non-linearity is stood in for by a cubic soft clip on the
band-pass integrator. That puts the right *cost* into the M0 benchmark; the
sound is calibrated in M1.

**Files:**
- Create: `core/sid/sid_math.h`, `core/sid/sid_filter.h`, `core/sid/sid_filter.c`
- Create: `core/sid/sid_curves_resid016.h`, with **its own licence**:
  GPL-2.0-or-later and copyright Dag Lem, because it holds reSID's data.
- Create: `LICENSES/GPL-2.0-or-later.txt`
- Create: `tests/unit/sid_filter_test.cpp`
- Modify: `core/CMakeLists.txt`, `tests/CMakeLists.txt`

**Step 1: Fetch the GPL-2.0 text**

Run: `curl -sSfL https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt -o LICENSES/GPL-2.0-or-later.txt`

**Step 2: Write the failing tests**

```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>

extern "C" {
#include "sid/sid_filter.h"
#include "sid/sid_math.h"
}

namespace {

constexpr float kFs = 44100.0f;

sid_svf linear_svf(float hz, float q) {
    sid_svf f;
    sid_svf_reset(&f, 0.0f);
    sid_svf_set(&f, sidou_tanf(SIDOU_PI * hz / kFs), 1.0f / q);
    return f;
}

} // namespace

TEST(SidMath, TanMatchesLibmOnTheCutoffRange) {
    for (float x = 0.0f; x < 1.35f; x += 0.01f) {
        EXPECT_NEAR(sidou_tanf(x), std::tan(x), 2e-5f * std::tan(x) + 1e-7f) << "x=" << x;
    }
}

TEST(SidFilter, LowPassPassesDcAndHighPassBlocksIt) {
    sid_svf f = linear_svf(1000.0f, 0.707f);
    for (int i = 0; i < 44100; ++i) {
        sid_svf_tick(&f, 0.5f);
    }
    EXPECT_NEAR(f.lp, 0.5f, 1e-4f);
    EXPECT_NEAR(f.hp, 0.0f, 1e-4f);
}

TEST(SidFilter, LowPassGainAtCutoffEqualsQ) {
    const float hz = 1000.0f, q = 0.707f;
    sid_svf f = linear_svf(hz, q);
    float peak = 0.0f;
    for (int i = 0; i < 44100; ++i) {
        float x = std::sin(2.0f * SIDOU_PI * hz * (float)i / kFs);
        sid_svf_tick(&f, x);
        if (i > 22050) {
            peak = std::fmax(peak, std::fabs(f.lp));
        }
    }
    EXPECT_NEAR(peak, q, 0.01f);
}

TEST(SidFilter, SoftClipBoundsTheBandPassState) {
    sid_svf f;
    sid_svf_reset(&f, 2.0f);
    sid_svf_set(&f, sidou_tanf(SIDOU_PI * 2000.0f / kFs), 1.0f / 3.0f);
    for (int i = 0; i < 4410; ++i) {
        sid_svf_tick(&f, (i / 20) % 2 ? 10.0f : -10.0f);
        ASSERT_LE(std::fabs(f.ic1), 1.0f / 2.0f + 1e-6f);
    }
}

TEST(SidCutoff, CurvesHitTheMeasuredPoints) {
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 0), 220.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 1023), 6000.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 1024), 4600.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 2047), 18000.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_8580, 2047), 12500.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_8580, 64), 400.0f);
}
```

**Step 3: Implement**

`core/sid/sid_math.h`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_MATH_H
#define SIDOU_SID_MATH_H

#define SIDOU_PI 3.14159265358979f

/* tan for 0 <= x < ~1.4 without libm (the firmware has none). Taylor series of sin and
 * cos; error < 1e-6 relative on that range. Pure float ops, so host and target agree. */
static inline float sidou_tanf(float x) {
    float x2 = x * x;
    float s = x * (1.0f - x2 / 6.0f * (1.0f - x2 / 20.0f * (1.0f - x2 / 42.0f *
                                                                    (1.0f - x2 / 72.0f *
                                                                                (1.0f - x2 / 110.0f)))));
    float c = 1.0f - x2 / 2.0f * (1.0f - x2 / 12.0f * (1.0f - x2 / 30.0f *
                                                             (1.0f - x2 / 56.0f *
                                                                         (1.0f - x2 / 90.0f *
                                                                                     (1.0f - x2 / 132.0f)))));
    return s / c;
}

/* Cubic soft clip: identity near 0, flat at +-1 beyond +-1.5. */
static inline float sidou_soft_clip(float x) {
    if (x > 1.5f) {
        return 1.0f;
    }
    if (x < -1.5f) {
        return -1.0f;
    }
    return x - (4.0f / 27.0f) * x * x * x;
}

#endif
```

`core/sid/sid_filter.h`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_FILTER_H
#define SIDOU_SID_FILTER_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_math.h"

/* Cytomic trapezoidal SVF. drive > 0 adds the 6581 band-pass saturation. */
typedef struct {
    float ic1, ic2;
    float lp, bp, hp;
    float a1, a2, a3, k;
    float drive, inv_drive;
} sid_svf;

void sid_svf_reset(sid_svf* f, float drive);
void sid_svf_set(sid_svf* f, float g, float k);
float sid_cutoff_hz(sid_model model, uint32_t fc);

static inline void sid_svf_tick(sid_svf* f, float v0) {
    float v3 = v0 - f->ic2;
    float v1 = f->a1 * f->ic1 + f->a2 * v3;
    float v2 = f->ic2 + f->a2 * f->ic1 + f->a3 * v3;
    f->ic1 = 2.0f * v1 - f->ic1;
    f->ic2 = 2.0f * v2 - f->ic2;
    if (f->drive > 0.0f) {
        f->ic1 = sidou_soft_clip(f->ic1 * f->drive) * f->inv_drive;
    }
    f->lp = v2;
    f->bp = v1;
    f->hp = v0 - f->k * v1 - v2;
}

#endif
```

`core/sid/sid_curves_resid016.h`:
```c
// SPDX-FileCopyrightText: 2004 Dag Lem <resid@nimrod.no>
// SPDX-License-Identifier: GPL-2.0-or-later
/* Filter cutoff measurements (FC register -> Hz) from reSID 0.16 filter.cc,
 * with its duplicated spline end points removed. */
#ifndef SIDOU_SID_CURVES_RESID016_H
#define SIDOU_SID_CURVES_RESID016_H

static const float SID_F0_POINTS_6581[][2] = {
    {0, 220},     {128, 230},   {256, 250},   {384, 300},   {512, 420},   {640, 780},
    {768, 1600},  {832, 2300},  {896, 3200},  {960, 4300},  {992, 5000},  {1008, 5400},
    {1016, 5700}, {1023, 6000}, {1024, 4600}, {1032, 4800}, {1056, 5300}, {1088, 6000},
    {1120, 6600}, {1152, 7200}, {1280, 9500}, {1408, 12000}, {1536, 14500}, {1664, 16000},
    {1792, 17100}, {1920, 17700}, {2047, 18000}};

static const float SID_F0_POINTS_8580[][2] = {
    {0, 0},       {128, 800},   {256, 1600},  {384, 2500},  {512, 3300},  {640, 4100},
    {768, 4800},  {896, 5600},  {1024, 6500}, {1152, 7500}, {1280, 8400}, {1408, 9200},
    {1536, 9800}, {1664, 10500}, {1792, 11000}, {1920, 11700}, {2047, 12500}};

#endif
```

`core/sid/sid_filter.c`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_filter.h"

#include "sid_curves_resid016.h"

#define COUNT(a) (sizeof(a) / sizeof((a)[0]))

void sid_svf_reset(sid_svf* f, float drive) {
    f->ic1 = f->ic2 = 0.0f;
    f->lp = f->bp = f->hp = 0.0f;
    f->a1 = 1.0f;
    f->a2 = f->a3 = 0.0f;
    f->k = 1.0f;
    f->drive = drive;
    f->inv_drive = drive > 0.0f ? 1.0f / drive : 0.0f;
}

void sid_svf_set(sid_svf* f, float g, float k) {
    f->k = k;
    f->a1 = 1.0f / (1.0f + g * (g + k));
    f->a2 = g * f->a1;
    f->a3 = g * f->a2;
}

static float interpolate(const float (*points)[2], uint32_t n, uint32_t fc) {
    for (uint32_t i = 1; i < n; ++i) {
        float x0 = points[i - 1][0], x1 = points[i][0];
        if ((float)fc <= x1) {
            float t = ((float)fc - x0) / (x1 - x0);
            return points[i - 1][1] + t * (points[i][1] - points[i - 1][1]);
        }
    }
    return points[n - 1][1];
}

float sid_cutoff_hz(sid_model model, uint32_t fc) {
    return model == SID_MODEL_6581
               ? interpolate(SID_F0_POINTS_6581, COUNT(SID_F0_POINTS_6581), fc)
               : interpolate(SID_F0_POINTS_8580, COUNT(SID_F0_POINTS_8580), fc);
}
```

Note: the 6581 table jumps from 1023 to 1024, so `interpolate` must hit
`x1 == 1023` exactly for fc 1023 and move on to the `{1024, 4600}` segment
for fc 1024. The `<=` comparison does that.

`core/CMakeLists.txt` now builds a real library:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_core STATIC
    sid/sid_filter.c)
target_include_directories(sidou_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
# Float results must be identical on host and pi32v2 so their outputs can be compared.
target_compile_options(sidou_core PUBLIC -ffp-contract=off)
target_compile_options(sidou_core PRIVATE -Wall -Wextra -Wshadow)
```

Add `unit/sid_filter_test.cpp` to the test sources.

**Step 4: Run the tests**

Run: `cmake --build --preset host && ctest --preset host -R "SidMath|SidFilter|SidCutoff"`
Expected: PASS. If `SidCutoff` fails at 64 → 400 for the 8580 (halfway
between 0 and 800), check the interpolation.

**Step 5: Commit**

```bash
git add core tests LICENSES/GPL-2.0-or-later.txt
git commit -m "feat(sid): trapezoidal SVF, 6581 saturation stand-in, reSID cutoff curves"
```

---

### Task 7: The chip: registers, voices, mixer and output

**Files:**
- Create: `core/sid/sid.h`, `core/sid/sid.c`
- Create: `tests/unit/sid_test.cpp`
- Modify: `core/CMakeLists.txt`, `tests/CMakeLists.txt`

**Step 1: Write the failing tests**

```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdlib>
#include <vector>

extern "C" {
#include "sid/sid.h"
}

namespace {

std::vector<int16_t> render(sid* s, uint32_t n) {
    std::vector<int16_t> out(n);
    sid_render(s, out.data(), n);
    return out;
}

int peak(const std::vector<int16_t>& v) {
    int p = 0;
    for (int16_t x : v) {
        p = std::max(p, std::abs((int)x));
    }
    return p;
}

void saw_a4(sid* s) {
    sid_write(s, 0x00, 0x45); // 440 Hz at PAL: 0x1d45
    sid_write(s, 0x01, 0x1d);
    sid_write(s, 0x05, 0x00);
    sid_write(s, 0x06, 0xf0);
    sid_write(s, 0x18, 0x0f);
    sid_write(s, 0x04, 0x21);
}

} // namespace

TEST(Sid, CyclesPerSampleIsPalClockOverSampleRate) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    EXPECT_EQ(s.cycles_per_sample_q16, (uint32_t)(((uint64_t)SID_CLOCK_PAL << 16) / 44100));
}

TEST(Sid, An8580IsSilentAfterInit) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    EXPECT_EQ(peak(render(&s, 4410)), 0);
}

TEST(Sid, GatedSawProducesUnclippedSound) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    saw_a4(&s);
    auto out = render(&s, 4410);
    EXPECT_GT(peak(out), 1000);
    EXPECT_LT(peak(out), 32767);
}

TEST(Sid, VolumeWriteOnA6581ClicksThenSettles) {
    static sid s;
    sid_init(&s, SID_MODEL_6581, SID_CLOCK_PAL, 44100);
    sid_write(&s, 0x18, 0x0f);
    auto click = render(&s, 10);
    EXPECT_GT(peak(click), 1000);
    render(&s, 44100);
    EXPECT_LT(peak(render(&s, 100)), 50);
}

TEST(Sid, Voice3OffMutesAnUnfilteredVoice3) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    sid_write(&s, 0x0e, 0x45);
    sid_write(&s, 0x0f, 0x1d);
    sid_write(&s, 0x14, 0xf0);
    sid_write(&s, 0x18, 0x8f);
    sid_write(&s, 0x12, 0x21);
    EXPECT_EQ(peak(render(&s, 4410)), 0);
}

TEST(Sid, LowPassFilterRemovesTopEnd) {
    static sid a, b;
    for (sid* s : {&a, &b}) {
        sid_init(s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
        saw_a4(s);
    }
    sid_write(&b, 0x16, 0x10); // fc ~ 128 -> 800 Hz
    sid_write(&b, 0x17, 0x01); // voice 1 through the filter
    sid_write(&b, 0x18, 0x1f); // low-pass, volume 15
    auto dry = render(&a, 4410), wet = render(&b, 4410);
    auto hf_energy = [](const std::vector<int16_t>& v) {
        double e = 0;
        for (size_t i = 1; i < v.size(); ++i) {
            double d = (double)v[i] - v[i - 1];
            e += d * d;
        }
        return e;
    };
    EXPECT_LT(hf_energy(wet), hf_energy(dry) * 0.1);
}
```

**Step 2: Run them and watch them fail**

Run: `cmake --build --preset host`
Expected: a compile error, `sid.h: No such file or directory`

**Step 3: Implement**

`core/sid/sid.h`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_H
#define SIDOU_SID_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_env.h"
#include "sid_filter.h"
#include "sid_osc.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FC_STEPS 2048

/* One SID chip. Fields are private to sid.c; the struct is public so callers can
 * allocate it statically. */
typedef struct {
    sid_osc osc[3];
    sid_env env[3];
    sid_svf svf;
    sid_model model;
    uint32_t cycles_per_sample_q16;
    uint32_t cycle_frac_q16;
    int32_t wave_zero;
    int32_t voice_dc;
    uint32_t fc;
    uint8_t res;
    uint8_t routing;
    uint8_t mode_vol;
    float volume;
    float dc_r, dc_x1, dc_y1;
    float cutoff_g[SID_FC_STEPS];
} sid;

/* sample_rate must be <= 65535. */
void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate);
void sid_write(sid* s, uint8_t reg, uint8_t value);
void sid_render(sid* s, int16_t* out, uint32_t frames);

#ifdef __cplusplus
}
#endif

#endif
```

`core/sid/sid.c`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid.h"

#include <string.h>

#define VOICE_SCALE (1.0f / (2048.0f * 255.0f))
#define OUTPUT_GAIN (0.1f * 32767.0f) /* leaves headroom for the 6581 DC offset and filter peaks */
#define DC_BLOCK_HZ 16.0f
#define DRIVE_6581 2.0f

static void update_filter(sid* s) {
    float q = 0.707f + (float)s->res / 15.0f;
    sid_svf_set(&s->svf, s->cutoff_g[s->fc], 1.0f / q);
}

void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate) {
    memset(s, 0, sizeof *s);
    s->model = model;
    /* reSID 0.16 voice DAC zero level and DC offset per chip revision. */
    s->wave_zero = model == SID_MODEL_6581 ? 0x380 : 0x800;
    s->voice_dc = model == SID_MODEL_6581 ? 0x800 * 0xff : 0;
    s->cycles_per_sample_q16 = ((clock_hz / sample_rate) << 16) |
                               (((clock_hz % sample_rate) << 16) / sample_rate);
    for (int v = 0; v < 3; ++v) {
        sid_osc_reset(&s->osc[v]);
        sid_env_reset(&s->env[v]);
    }
    float limit = 0.45f * (float)sample_rate;
    for (uint32_t fc = 0; fc < SID_FC_STEPS; ++fc) {
        float hz = sid_cutoff_hz(model, fc);
        if (hz < 30.0f) {
            hz = 30.0f;
        }
        if (hz > limit) {
            hz = limit;
        }
        s->cutoff_g[fc] = sidou_tanf(SIDOU_PI * hz / (float)sample_rate);
    }
    sid_svf_reset(&s->svf, model == SID_MODEL_6581 ? DRIVE_6581 : 0.0f);
    s->dc_r = 1.0f - 2.0f * SIDOU_PI * DC_BLOCK_HZ / (float)sample_rate;
    update_filter(s);
}

void sid_write(sid* s, uint8_t reg, uint8_t v) {
    reg &= 0x1f;
    if (reg < 21) {
        sid_osc* o = &s->osc[reg / 7];
        sid_env* e = &s->env[reg / 7];
        switch (reg % 7) {
        case 0: o->freq = (o->freq & 0xff00u) | v; break;
        case 1: o->freq = (o->freq & 0x00ffu) | ((uint32_t)v << 8); break;
        case 2: o->pw = (o->pw & 0xf00u) | v; break;
        case 3: o->pw = (o->pw & 0x0ffu) | ((uint32_t)(v & 0x0f) << 8); break;
        case 4:
            sid_osc_write_control(o, v);
            sid_env_set_gate(e, v & SID_CTRL_GATE);
            break;
        case 5: sid_env_write_ad(e, v); break;
        case 6: sid_env_write_sr(e, v); break;
        }
        return;
    }
    switch (reg) {
    case 0x15:
        s->fc = (s->fc & 0x7f8u) | (v & 0x07u);
        update_filter(s);
        break;
    case 0x16:
        s->fc = (s->fc & 0x007u) | ((uint32_t)v << 3);
        update_filter(s);
        break;
    case 0x17:
        s->res = v >> 4;
        s->routing = v & 0x0f;
        update_filter(s);
        break;
    case 0x18:
        s->mode_vol = v;
        s->volume = (float)(v & 0x0f) / 15.0f;
        break;
    default: break;
    }
}

static inline float voice_out(const sid* s, int v) {
    uint32_t wave = sid_osc_output(&s->osc[v], &s->osc[(v + 2) % 3]);
    int32_t x = ((int32_t)wave - s->wave_zero) * (int32_t)s->env[v].counter + s->voice_dc;
    return (float)x * VOICE_SCALE;
}

void sid_render(sid* s, int16_t* out, uint32_t frames) {
    for (uint32_t i = 0; i < frames; ++i) {
        s->cycle_frac_q16 += s->cycles_per_sample_q16;
        uint32_t cycles = s->cycle_frac_q16 >> 16;
        s->cycle_frac_q16 &= 0xffffu;

        for (int v = 0; v < 3; ++v) {
            sid_osc_advance(&s->osc[v], cycles);
        }
        for (int v = 0; v < 3; ++v) {
            const sid_osc* src = &s->osc[(v + 2) % 3];
            if ((s->osc[v].control & SID_CTRL_SYNC) && src->msb_rose) {
                sid_osc_sync(&s->osc[v], src);
            }
            sid_env_advance(&s->env[v], cycles);
        }

        float filt_in = 0.0f, direct = 0.0f;
        for (int v = 0; v < 3; ++v) {
            float x = voice_out(s, v);
            if (s->routing & (1u << v)) {
                filt_in += x;
            } else if (!(v == 2 && (s->mode_vol & 0x80u))) {
                direct += x;
            }
        }
        sid_svf_tick(&s->svf, filt_in);
        float filtered = 0.0f;
        if (s->mode_vol & 0x10u) {
            filtered += s->svf.lp;
        }
        if (s->mode_vol & 0x20u) {
            filtered += s->svf.bp;
        }
        if (s->mode_vol & 0x40u) {
            filtered += s->svf.hp;
        }

        float x = (direct + filtered) * s->volume;
        float y = x - s->dc_x1 + s->dc_r * s->dc_y1;
        s->dc_x1 = x;
        s->dc_y1 = y;

        float o = y * OUTPUT_GAIN;
        if (o > 32767.0f) {
            o = 32767.0f;
        } else if (o < -32768.0f) {
            o = -32768.0f;
        }
        out[i] = (int16_t)o;
    }
}
```

Add `sid/sid.c` to `sidou_core` and `unit/sid_test.cpp` to the tests.

**Step 4: Run all tests, including under the sanitizers**

Run: `cmake --build --preset host && ctest --preset host`
Expected: everything passes.

Run: `cmake --preset host-asan && cmake --build --preset host-asan && ctest --preset host-asan`
Expected: everything passes, with no ASan or UBSan reports.

**Step 5: Commit**

```bash
git add core tests
git commit -m "feat(sid): chip glue with register map, mixer, DC blocker and output"
```

---

### Task 8: Shared benchmark workload and golden-render tests

One workload, used by four consumers: the golden tests, the host benchmark,
the pi32v2 benchmark and the Rust emulator test. It is the worst case for
cost: three voices, all through the filter, with a cutoff sweep, noise, and
a 6581 model with saturation.

**Files:**
- Create: `bench/CMakeLists.txt`, `bench/workload.h`, `bench/workload.c`
- Create: `tests/golden/golden_test.cpp`, `tests/golden/hashes.txt` (generated)
- Modify: `CMakeLists.txt`, `tests/CMakeLists.txt`, `REUSE.toml`

**Step 1: Write the workload**

`bench/workload.h`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_WORKLOAD_H
#define SIDOU_WORKLOAD_H

#include <stdint.h>

#include "sid/sid_defs.h"

#ifdef __cplusplus
extern "C" {
#endif

#define WORKLOAD_SAMPLE_RATE 44100u
#define WORKLOAD_SAMPLES 11025u
#define WORKLOAD_MAX_CHIPS 2u

/* Chips beyond the first replay the same register writes and are mixed in at
 * equal level. */
void workload_begin(uint32_t chips, sid_model model);
void workload_render_samples(int16_t* out, uint32_t n);
uint32_t workload_sample_count(void);

#ifdef __cplusplus
}
#endif

#endif
```

`bench/workload.c`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "workload.h"

#include "sid/sid.h"

#define SAMPLES_PER_FRAME (WORKLOAD_SAMPLE_RATE / 50u)
#define BLOCK 128u

typedef struct {
    uint16_t frame;
    uint8_t reg;
    uint8_t value;
} reg_write;

/* C major triad (PAL frequencies), every voice through a resonant low-pass;
 * the cutoff opens at frame 3, voice 3 turns to noise at frame 6, all gates
 * close at frame 9. */
static const reg_write WRITES[] = {
    {0, 0x18, 0x1f}, {0, 0x17, 0x87}, {0, 0x15, 0x00}, {0, 0x16, 0x40},
    {0, 0x00, 0x67}, {0, 0x01, 0x11}, {0, 0x05, 0x09}, {0, 0x06, 0xa9}, {0, 0x04, 0x21},
    {0, 0x07, 0xed}, {0, 0x08, 0x15}, {0, 0x09, 0x00}, {0, 0x0a, 0x08}, {0, 0x0c, 0x09},
    {0, 0x0d, 0xa9}, {0, 0x0b, 0x41},
    {0, 0x0e, 0x13}, {0, 0x0f, 0x1a}, {0, 0x13, 0x09}, {0, 0x14, 0xa9}, {0, 0x12, 0x11},
    {3, 0x16, 0x80}, {6, 0x12, 0x81},
    {9, 0x04, 0x20}, {9, 0x0b, 0x40}, {9, 0x12, 0x80},
};

static sid chips[WORKLOAD_MAX_CHIPS];
static uint32_t chip_count;
static uint32_t position;
static uint32_t next_write;

void workload_begin(uint32_t count, sid_model model) {
    chip_count = count > WORKLOAD_MAX_CHIPS ? WORKLOAD_MAX_CHIPS : count;
    position = 0;
    next_write = 0;
    for (uint32_t c = 0; c < chip_count; ++c) {
        sid_init(&chips[c], model, SID_CLOCK_PAL, WORKLOAD_SAMPLE_RATE);
    }
}

void workload_render_samples(int16_t* out, uint32_t n) {
    int16_t mix[BLOCK];
    while (n) {
        uint32_t frame = position / SAMPLES_PER_FRAME;
        while (next_write < sizeof WRITES / sizeof WRITES[0] && WRITES[next_write].frame <= frame) {
            for (uint32_t c = 0; c < chip_count; ++c) {
                sid_write(&chips[c], WRITES[next_write].reg, WRITES[next_write].value);
            }
            ++next_write;
        }
        uint32_t len = SAMPLES_PER_FRAME - position % SAMPLES_PER_FRAME;
        if (len > BLOCK) {
            len = BLOCK;
        }
        if (len > n) {
            len = n;
        }
        sid_render(&chips[0], out, len);
        for (uint32_t c = 1; c < chip_count; ++c) {
            sid_render(&chips[c], mix, len);
            for (uint32_t i = 0; i < len; ++i) {
                out[i] = (int16_t)(((int32_t)out[i] + mix[i]) / 2);
            }
        }
        out += len;
        n -= len;
        position += len;
    }
}

uint32_t workload_sample_count(void) { return WORKLOAD_SAMPLES; }
```

`bench/CMakeLists.txt`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_workload STATIC workload.c)
target_include_directories(sidou_workload PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_workload PUBLIC sidou_core)
```

In the top-level `CMakeLists.txt`, add `add_subdirectory(bench)` after
`add_subdirectory(core)`.

**Step 2: Write the golden test**

`tests/golden/golden_test.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>
#include <cstdlib>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

extern "C" {
#include "workload.h"
}

namespace {

struct Case {
    std::string name;
    uint32_t chips;
    sid_model model;
};

const Case kCases[] = {
    {"workload_6581_1chip", 1, SID_MODEL_6581},
    {"workload_8580_1chip", 1, SID_MODEL_8580},
    {"workload_6581_2chip", 2, SID_MODEL_6581},
};

std::vector<int16_t> render(const Case& c) {
    std::vector<int16_t> out(workload_sample_count());
    workload_begin(c.chips, c.model);
    workload_render_samples(out.data(), (uint32_t)out.size());
    return out;
}

uint64_t fnv1a(const std::vector<int16_t>& samples) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (int16_t s : samples) {
        uint16_t u = (uint16_t)s;
        for (int byte = 0; byte < 2; ++byte) {
            h ^= (u >> (8 * byte)) & 0xffu;
            h *= 0x100000001b3ull;
        }
    }
    return h;
}

std::map<std::string, std::string> load(const char* path) {
    std::map<std::string, std::string> hashes;
    std::ifstream in(path);
    std::string name, hash;
    while (in >> name >> hash) {
        hashes[name] = hash;
    }
    return hashes;
}

std::string hex(uint64_t v) {
    std::ostringstream s;
    s << std::hex << v;
    return s.str();
}

bool updating() {
    const char* v = std::getenv("SIDOU_GOLDEN_UPDATE");
    return v && std::string(v) == "1";
}

} // namespace

TEST(Golden, RendersMatchRecordedHashes) {
    if (updating()) {
        std::ofstream out(SIDOU_GOLDEN_FILE);
        for (const Case& c : kCases) {
            out << c.name << ' ' << hex(fnv1a(render(c))) << '\n';
        }
        GTEST_SKIP() << "rewrote " << SIDOU_GOLDEN_FILE;
    }
    auto expected = load(SIDOU_GOLDEN_FILE);
    for (const Case& c : kCases) {
        ASSERT_TRUE(expected.count(c.name))
            << c.name << " missing; run with SIDOU_GOLDEN_UPDATE=1 and review the diff";
        EXPECT_EQ(hex(fnv1a(render(c))), expected[c.name])
            << c.name << " changed. If intended, rerun with SIDOU_GOLDEN_UPDATE=1";
    }
}

TEST(Golden, WorkloadNeverClipsAndHasNoDcOffset) {
    // The 6581's volume write at frame 0 is a real DC step; the 16 Hz blocker needs
    // ~50 ms to absorb it, so the mean is taken after that.
    const size_t settle = 2205;
    for (const Case& c : kCases) {
        auto out = render(c);
        double sum = 0;
        for (size_t i = 0; i < out.size(); ++i) {
            ASSERT_GT(out[i], -32768) << c.name;
            ASSERT_LT(out[i], 32767) << c.name;
            if (i >= settle) {
                sum += out[i];
            }
        }
        EXPECT_LT(std::fabs(sum / (double)(out.size() - settle)), 200.0) << c.name;
    }
}
```

Register it in `tests/CMakeLists.txt`:
```cmake
add_executable(sidou_golden_tests golden/golden_test.cpp)
target_link_libraries(sidou_golden_tests PRIVATE sidou_workload GTest::gtest_main)
target_compile_definitions(sidou_golden_tests PRIVATE
    SIDOU_GOLDEN_FILE="${CMAKE_CURRENT_SOURCE_DIR}/golden/hashes.txt")
gtest_discover_tests(sidou_golden_tests)
```

Add `tests/golden/hashes.txt` to the `REUSE.toml` annotation list.

**Step 3: Run it and watch it fail**

Run: `cmake --build --preset host && ctest --preset host -R Golden`
Expected: `RendersMatchRecordedHashes` FAILS with "missing; run with
SIDOU_GOLDEN_UPDATE=1". `WorkloadNeverClipsAndHasNoDcOffset` PASSES. A pre-check of
this plan measured the workload peak at 12541 (6581) and 10428 (8580), with a
settled mean of about -103.

**Step 4: Record the hashes, then check they're stable on both sanitizer and optimised builds**

Run: `SIDOU_GOLDEN_UPDATE=1 ./build/host/tests/sidou_golden_tests`
Run: `ctest --preset host -R Golden && cmake --build --preset host-asan && ctest --preset host-asan -R Golden`
Expected: PASS on both presets. The pre-check (Apple clang, arm64) recorded
`workload_6581_1chip bc496c58a4b660eb` and `workload_8580_1chip 95bafea914fe070a`.
Those hashes are superseded: review fixes made after the pre-check (output
rounding, denormal flushes) changed the output, so `tests/golden/hashes.txt` is
the reference. The 2-chip hash differs from the 1-chip one, because chip 1
plays voice 1 at a raised frequency. If yours differ on another compiler or architecture, that is a
determinism bug worth chasing. If the Debug and RelWithDebInfo hashes
differ, float evaluation isn't deterministic. Find out why (check
`-ffp-contract=off` is on the compile line in
`build/*/compile_commands.json`) before going any further.

**Step 5: Commit**

```bash
git add bench tests CMakeLists.txt REUSE.toml
git commit -m "test: shared SID workload with golden render hashes"
```

---

### Task 9: Host micro-benchmark with Google Benchmark

The host benchmark is for quick comparisons while developing. It doesn't
decide anything; the target number from Task 13 does.

**Files:**
- Create: `bench/host/CMakeLists.txt`, `bench/host/sid_bench.cpp`
- Modify: `bench/CMakeLists.txt`

**Step 1: Write it**

`bench/host/CMakeLists.txt`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
include(FetchContent)
FetchContent_Declare(googlebenchmark
    GIT_REPOSITORY https://github.com/google/benchmark.git
    GIT_TAG v1.9.1
    GIT_SHALLOW TRUE)
set(BENCHMARK_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
set(BENCHMARK_ENABLE_GTEST_TESTS OFF CACHE BOOL "" FORCE)
set(BENCHMARK_ENABLE_INSTALL OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googlebenchmark)

add_executable(sidou_bench sid_bench.cpp)
target_link_libraries(sidou_bench PRIVATE sidou_workload benchmark::benchmark_main)
```

`bench/host/sid_bench.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <benchmark/benchmark.h>

#include <vector>

extern "C" {
#include "workload.h"
}

static void BM_Workload(benchmark::State& state) {
    const auto chips = (uint32_t)state.range(0);
    const auto model = (sid_model)state.range(1);
    std::vector<int16_t> out(workload_sample_count());
    for (auto _ : state) {
        state.PauseTiming();
        workload_begin(chips, model);
        state.ResumeTiming();
        workload_render_samples(out.data(), (uint32_t)out.size());
        benchmark::DoNotOptimize(out.data());
    }
    state.SetItemsProcessed(state.iterations() * (int64_t)out.size());
    state.counters["chips"] = (double)chips;
}
BENCHMARK(BM_Workload)
    ->ArgsProduct({{1, 2}, {SID_MODEL_6581, SID_MODEL_8580}})
    ->ArgNames({"chips", "model"})
    ->Unit(benchmark::kMicrosecond);
```

In `bench/CMakeLists.txt` add:
```cmake
if(CMAKE_CROSSCOMPILING)
    add_subdirectory(fm1)
else()
    add_subdirectory(host)
endif()
```
The `fm1` subdirectory doesn't exist until Task 12. Until then, wrap the
`add_subdirectory(fm1)` line in
`if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/fm1)`, or add both together in Task 12.

**Step 2: Run it**

Run: `cmake --build --preset host && ./build/host/bench/host/sidou_bench`
Expected: four rows. `items_per_second` is samples per second; keep a note
of it for the decision record.

**Step 3: Commit**

```bash
git add bench
git commit -m "bench: Google Benchmark host micro-benchmark of the workload"
```

---

### Task 10: reSIDfp reference harness

reSIDfp is the reference we compare against. M0 only *asserts* what should
already match: pitch, and envelope shape with the filter off. Filter accuracy
is measured, written to WAV files and *reported* by a separate compare tool;
M1 turns those measurements into assertions.

**Files:**
- Create: `tests/reference/CMakeLists.txt`
- Create: `tests/reference/residfp_engine.h`, `tests/reference/residfp_engine.cpp`
- Create: `tests/reference/render.h`, `tests/reference/render.cpp`
- Create: `tests/reference/signal_metrics.h`, `tests/reference/signal_metrics.cpp`
- Create: `tests/reference/reference_test.cpp`
- Create: `tests/reference/sidou_compare.cpp`
- Modify: `tests/CMakeLists.txt` (add `add_subdirectory(reference)`)

**Step 1: Build libresidfp from a pinned commit**

`tests/reference/CMakeLists.txt`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# reSIDfp (GPL-2.0-or-later) is a test-only reference. It is never linked into firmware.
include(FetchContent)
FetchContent_Declare(libresidfp
    GIT_REPOSITORY https://github.com/libsidplayfp/libresidfp.git
    GIT_TAG c1791383ca843a855e5d4366da6a7277e34e801d)
FetchContent_MakeAvailable(libresidfp)

set(RFP ${libresidfp_SOURCE_DIR}/src)
set(RFP_GEN ${CMAKE_CURRENT_BINARY_DIR}/residfp-gen)
set(RESIDFP_INLINING 1)
set(RESIDFP_INLINE inline)
set(RESIDFP_BRANCH_HINTS 1)
set(HAVE_BUILTIN_EXPECT 1)
set(PACKAGE_VERSION git)
set(LIB_MAJOR 3)
set(LIB_MINOR 0)
set(LIB_LEVEL 0)
configure_file(${RFP}/siddefs-fp.h.in ${RFP_GEN}/siddefs-fp.h @ONLY)
configure_file(${RFP}/residfp/sidversion.h.in ${RFP_GEN}/residfp/sidversion.h @ONLY)

add_library(residfp STATIC
    ${RFP}/Dac.cpp ${RFP}/EnvelopeGenerator.cpp ${RFP}/ExternalFilter.cpp ${RFP}/Filter.cpp
    ${RFP}/Filter6581.cpp ${RFP}/Filter8580.cpp ${RFP}/FilterModelConfig.cpp
    ${RFP}/FilterModelConfig6581.cpp ${RFP}/FilterModelConfig8580.cpp ${RFP}/Integrator6581.cpp
    ${RFP}/Integrator8580.cpp ${RFP}/OpAmp.cpp ${RFP}/SID.cpp ${RFP}/Spline.cpp ${RFP}/State.cpp
    ${RFP}/WaveformCalculator.cpp ${RFP}/WaveformGenerator.cpp ${RFP}/resample/SincResampler.cpp)
target_include_directories(residfp PUBLIC ${RFP} ${RFP_GEN})
target_compile_options(residfp PRIVATE -w)
find_package(Threads REQUIRED)
target_link_libraries(residfp PUBLIC Threads::Threads)

add_library(sidou_reference STATIC residfp_engine.cpp render.cpp signal_metrics.cpp)
target_include_directories(sidou_reference PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_reference PUBLIC residfp sidou_core)

add_executable(sidou_reference_tests reference_test.cpp)
target_link_libraries(sidou_reference_tests PRIVATE sidou_reference GTest::gtest_main)
gtest_discover_tests(sidou_reference_tests)

add_executable(sidou_compare sidou_compare.cpp)
target_link_libraries(sidou_compare PRIVATE sidou_reference)
```

Run: `cmake --preset host && cmake --build --preset host --target residfp`
Expected: it builds. If a header is missing, compare against the
`siddefs-fp.h.in` placeholders in the fetched source. The list above
(`RESIDFP_*`, `HAVE_BUILTIN_EXPECT`, `PACKAGE_VERSION`, `LIB_*`) matches the
pinned commit.

**Step 2: Engine wrappers and the renderer**

`tests/reference/residfp_engine.h`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstdint>
#include <memory>
#include <vector>

extern "C" {
#include "sid/sid_defs.h"
}

namespace reSIDfp {
class SID;
}

class ResidfpEngine {
public:
    /* exact_rate: reSIDfp's DECIMATE (linear interpolation) keeps the exact output
     * rate; its RESAMPLE (two-pass sinc) sounds better but its intermediate rate
     * makes it run ~0.02% fast (measured: 44110 samples per 44100 expected). */
    ResidfpEngine(sid_model model, double sample_rate, bool exact_rate);
    ~ResidfpEngine();
    void write(uint8_t reg, uint8_t value);
    void render_cycles(unsigned cycles, std::vector<int16_t>& out);

private:
    std::unique_ptr<reSIDfp::SID> sid_;
    double sample_rate_;
};
```

`tests/reference/residfp_engine.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "residfp_engine.h"

#include "SID.h"

ResidfpEngine::ResidfpEngine(sid_model model, double sample_rate, bool exact_rate)
    : sid_(std::make_unique<reSIDfp::SID>()), sample_rate_(sample_rate) {
    sid_->setChipModel(model == SID_MODEL_6581 ? reSIDfp::MOS6581 : reSIDfp::CSG8580);
    sid_->setSamplingParameters(SID_CLOCK_PAL, exact_rate ? reSIDfp::DECIMATE : reSIDfp::RESAMPLE,
                               sample_rate);
    sid_->reset();
}

ResidfpEngine::~ResidfpEngine() = default;

void ResidfpEngine::write(uint8_t reg, uint8_t value) { sid_->write(reg, value); }

void ResidfpEngine::render_cycles(unsigned cycles, std::vector<int16_t>& out) {
    std::vector<int16_t> buf((size_t)(cycles * sample_rate_ / SID_CLOCK_PAL) + 64);
    int n = sid_->clock(cycles, buf.data());
    out.insert(out.end(), buf.begin(), buf.begin() + n);
}
```

`tests/reference/render.h`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstdint>
#include <vector>

extern "C" {
#include "sid/sid_defs.h"
}

struct RegWrite {
    uint32_t frame;
    uint8_t reg;
    uint8_t value;
};

constexpr double kRefSampleRate = 44100.0;

/* Both renderers apply writes at PAL frame (50 Hz) boundaries and return
 * float samples in [-1, 1]. */
std::vector<float> render_sidou(sid_model model, const std::vector<RegWrite>& writes,
                                uint32_t frames);
/* exact_rate: see ResidfpEngine. Use it when timing or pitch is measured. */
std::vector<float> render_residfp(sid_model model, const std::vector<RegWrite>& writes,
                                  uint32_t frames, bool exact_rate = false);
```

`tests/reference/render.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "render.h"

#include <memory>

#include "residfp_engine.h"

extern "C" {
#include "sid/sid.h"
}

namespace {

constexpr uint32_t kSamplesPerFrame = 882;
constexpr unsigned kCyclesPerFrame = 19705; // 985248 / 50, rounded

template <typename Write>
size_t apply(const std::vector<RegWrite>& writes, size_t next, uint32_t frame, Write write) {
    while (next < writes.size() && writes[next].frame <= frame) {
        write(writes[next].reg, writes[next].value);
        ++next;
    }
    return next;
}

std::vector<float> to_float(const std::vector<int16_t>& in) {
    std::vector<float> out(in.size());
    for (size_t i = 0; i < in.size(); ++i) {
        out[i] = (float)in[i] / 32768.0f;
    }
    return out;
}

} // namespace

std::vector<float> render_sidou(sid_model model, const std::vector<RegWrite>& writes,
                                uint32_t frames) {
    auto s = std::make_unique<sid>();
    sid_init(s.get(), model, SID_CLOCK_PAL, (uint32_t)kRefSampleRate);
    std::vector<int16_t> out(frames * kSamplesPerFrame);
    size_t next = 0;
    for (uint32_t f = 0; f < frames; ++f) {
        next = apply(writes, next, f, [&](uint8_t r, uint8_t v) { sid_write(s.get(), r, v); });
        sid_render(s.get(), out.data() + f * kSamplesPerFrame, kSamplesPerFrame);
    }
    return to_float(out);
}

std::vector<float> render_residfp(sid_model model, const std::vector<RegWrite>& writes,
                                  uint32_t frames, bool exact_rate) {
    ResidfpEngine e(model, kRefSampleRate, exact_rate);
    std::vector<int16_t> out;
    size_t next = 0;
    for (uint32_t f = 0; f < frames; ++f) {
        next = apply(writes, next, f, [&](uint8_t r, uint8_t v) { e.write(r, v); });
        e.render_cycles(kCyclesPerFrame, out);
    }
    return to_float(out);
}
```

**Step 3: Signal metrics**

`tests/reference/signal_metrics.h`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstddef>
#include <string>
#include <vector>

/* Fundamental frequency as the peak of a Hann-windowed DFT magnitude searched
 * between f_lo and f_hi. Unlike autocorrelation it is not biased by how sharp
 * (aliased or band-limited) the waveform is. */
double estimate_f0(const std::vector<float>& x, double fs, double f_lo, double f_hi);

/* Mean-removed RMS per window, in dB (not normalised). */
std::vector<double> rms_envelope_db(const std::vector<float>& x, double fs, double window_ms);

/* Shift an envelope so that window `index` is 0 dB; SIZE_MAX means the loudest window. */
std::vector<double> normalized(std::vector<double> env, size_t index);

/* Largest |a - b| over windows from `first` on where the reference b is above floor_db. */
double max_envelope_diff_db(const std::vector<double>& a, const std::vector<double>& b,
                            double floor_db, size_t first);

void write_wav16(const std::string& path, const std::vector<float>& x, int fs);
```

`tests/reference/signal_metrics.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "signal_metrics.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <fstream>

namespace {

double dft_magnitude(const std::vector<float>& x, const std::vector<double>& window, double f,
                     double fs) {
    const double w = 2.0 * M_PI * f / fs;
    double re = 0, im = 0;
    for (size_t i = 0; i < x.size(); ++i) {
        re += window[i] * x[i] * std::cos(w * (double)i);
        im -= window[i] * x[i] * std::sin(w * (double)i);
    }
    return std::sqrt(re * re + im * im);
}

} // namespace

double estimate_f0(const std::vector<float>& x, double fs, double f_lo, double f_hi) {
    std::vector<double> window(x.size());
    for (size_t i = 0; i < x.size(); ++i) {
        window[i] = 0.5 - 0.5 * std::cos(2.0 * M_PI * (double)i / (double)(x.size() - 1));
    }
    // Coarse scan, then golden-section search around the best bin.
    const double step = fs / (double)x.size() / 4.0;
    double best_f = f_lo, best = -1;
    for (double f = f_lo; f <= f_hi; f += step) {
        double m = dft_magnitude(x, window, f, fs);
        if (m > best) {
            best = m;
            best_f = f;
        }
    }
    double a = best_f - step, b = best_f + step;
    const double g = (std::sqrt(5.0) - 1.0) / 2.0;
    double c = b - g * (b - a), d = a + g * (b - a);
    double mc = dft_magnitude(x, window, c, fs), md = dft_magnitude(x, window, d, fs);
    while (b - a > 1e-4) {
        if (mc > md) {
            b = d;
            d = c;
            md = mc;
            c = b - g * (b - a);
            mc = dft_magnitude(x, window, c, fs);
        } else {
            a = c;
            c = d;
            mc = md;
            d = a + g * (b - a);
            md = dft_magnitude(x, window, d, fs);
        }
    }
    return 0.5 * (a + b);
}

std::vector<double> rms_envelope_db(const std::vector<float>& x, double fs, double window_ms) {
    const size_t w = (size_t)(fs * window_ms / 1000.0);
    std::vector<double> db;
    for (size_t start = 0; start + w <= x.size(); start += w) {
        double mean = 0;
        for (size_t i = start; i < start + w; ++i) {
            mean += x[i];
        }
        mean /= (double)w;
        double sq = 0;
        for (size_t i = start; i < start + w; ++i) {
            sq += (x[i] - mean) * (x[i] - mean);
        }
        db.push_back(10.0 * std::log10(sq / (double)w + 1e-20));
    }
    return db;
}

std::vector<double> normalized(std::vector<double> env, size_t index) {
    const double ref = index == SIZE_MAX ? *std::max_element(env.begin(), env.end()) : env.at(index);
    for (double& v : env) {
        v -= ref;
    }
    return env;
}

double max_envelope_diff_db(const std::vector<double>& a, const std::vector<double>& b,
                            double floor_db, size_t first) {
    double worst = 0;
    for (size_t i = first; i < std::min(a.size(), b.size()); ++i) {
        if (b[i] > floor_db) {
            worst = std::max(worst, std::fabs(a[i] - b[i]));
        }
    }
    return worst;
}

void write_wav16(const std::string& path, const std::vector<float>& x, int fs) {
    std::ofstream f(path, std::ios::binary);
    auto u32 = [&](uint32_t v) { f.write(reinterpret_cast<const char*>(&v), 4); };
    auto u16 = [&](uint16_t v) { f.write(reinterpret_cast<const char*>(&v), 2); };
    const uint32_t bytes = (uint32_t)x.size() * 2;
    f.write("RIFF", 4);
    u32(36 + bytes);
    f.write("WAVEfmt ", 8);
    u32(16);
    u16(1);
    u16(1);
    u32((uint32_t)fs);
    u32((uint32_t)fs * 2);
    u16(2);
    u16(16);
    f.write("data", 4);
    u32(bytes);
    for (float s : x) {
        u16((uint16_t)(int16_t)std::clamp(s * 32767.0f, -32768.0f, 32767.0f));
    }
}
```

**Step 4: Write the reference tests**

`tests/reference/reference_test.cpp`:
```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>

#include "render.h"
#include "signal_metrics.h"

namespace {

/* M0 tolerances. Tighten them in M1; never loosen them to make a failure pass
 * without writing down why. */
constexpr double kMaxPitchCents = 0.1; /* measured at M0: 0.03 */
constexpr double kMaxEnvelopeDb8580 = 1.0; /* measured at M0: 0.64 */
/* reSIDfp models the 6581's non-linear envelope DAC, which M0 does not; the
 * release tail drifts ~2 dB apart by -20 dB. M1 adds the DAC and tightens this. */
constexpr double kMaxEnvelopeDb6581 = 2.5; /* measured at M0: 2.26 */
constexpr double kEnvelopeFloorDb = -20.0;
constexpr size_t kSkipWindows = 5;      /* first 50 ms: start-up transients */
constexpr size_t kSustainWindow = 30;   /* 300 ms, in the sustain phase */

class Reference : public ::testing::TestWithParam<sid_model> {};

std::vector<RegWrite> saw_a4(uint8_t ad, uint8_t sr, uint32_t gate_off_frame) {
    return {{0, 0x18, 0x0f}, {0, 0x00, 0x45}, {0, 0x01, 0x1d},
            {0, 0x05, ad},   {0, 0x06, sr},   {0, 0x04, 0x21},
            {gate_off_frame, 0x04, 0x20}};
}

} // namespace

TEST_P(Reference, UnfilteredSawPitchMatches) {
    auto writes = saw_a4(0x00, 0xf0, 1000);
    auto ours = render_sidou(GetParam(), writes, 60);
    auto ref = render_residfp(GetParam(), writes, 60, /*exact_rate=*/true);
    std::vector<float> a(ours.begin() + 4410, ours.begin() + 4410 + 44100);
    std::vector<float> b(ref.begin() + 4410, ref.begin() + 4410 + 44100);
    const double expected = 0x1d45 * (double)SID_CLOCK_PAL / 16777216.0;
    const double fa = estimate_f0(a, kRefSampleRate, 400, 480);
    const double fb = estimate_f0(b, kRefSampleRate, 400, 480);
    EXPECT_NEAR(fb, expected, 0.05) << "the reference itself is off; check the harness";
    EXPECT_LT(std::fabs(1200.0 * std::log2(fa / fb)), kMaxPitchCents)
        << "sidou " << fa << " Hz vs reSIDfp " << fb << " Hz";
}

TEST_P(Reference, EnvelopeShapeMatches) {
    auto writes = saw_a4(0x29, 0xa9, 20);
    auto ours = normalized(rms_envelope_db(render_sidou(GetParam(), writes, 50), kRefSampleRate, 10),
                           kSustainWindow);
    auto ref = normalized(rms_envelope_db(render_residfp(GetParam(), writes, 50), kRefSampleRate, 10),
                          kSustainWindow);
    const double tolerance = GetParam() == SID_MODEL_6581 ? kMaxEnvelopeDb6581 : kMaxEnvelopeDb8580;
    EXPECT_LT(max_envelope_diff_db(ours, ref, kEnvelopeFloorDb, kSkipWindows), tolerance);
}

INSTANTIATE_TEST_SUITE_P(Models, Reference,
                         ::testing::Values(SID_MODEL_6581, SID_MODEL_8580),
                         [](const auto& info) {
                             return info.param == SID_MODEL_6581 ? "MOS6581" : "CSG8580";
                         });
```

**Step 5: Write the compare tool**

`tests/reference/sidou_compare.cpp` renders filter scenarios through both
engines, writes WAV pairs for listening, and prints the envelope difference
per scenario. It isn't a test. It's the instrument M1 tunes against.

```cpp
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <cstdio>
#include <string>

#include "render.h"
#include "signal_metrics.h"

namespace {

struct Scenario {
    const char* name;
    std::vector<RegWrite> writes;
    uint32_t frames;
};

std::vector<RegWrite> noise_sweep(uint8_t mode) {
    std::vector<RegWrite> w = {{0, 0x18, (uint8_t)(mode | 0x0f)}, {0, 0x17, 0x81},
                               {0, 0x00, 0x00}, {0, 0x01, 0x40},
                               {0, 0x05, 0x00}, {0, 0x06, 0xf0}, {0, 0x04, 0x81}};
    for (uint32_t f = 0; f < 64; ++f) {
        w.push_back({f, 0x16, (uint8_t)(f * 4)});
    }
    return w;
}

} // namespace

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : ".";
    const Scenario scenarios[] = {
        {"lp_noise_sweep", noise_sweep(0x10), 64},
        {"bp_noise_sweep", noise_sweep(0x20), 64},
        {"hp_noise_sweep", noise_sweep(0x40), 64},
    };
    std::printf("%-16s %-8s %s\n", "scenario", "model", "max envelope diff (dB)");
    for (const auto& s : scenarios) {
        for (sid_model m : {SID_MODEL_6581, SID_MODEL_8580}) {
            const char* tag = m == SID_MODEL_6581 ? "6581" : "8580";
            auto ours = render_sidou(m, s.writes, s.frames);
            auto ref = render_residfp(m, s.writes, s.frames);
            write_wav16(dir + "/" + s.name + "_" + tag + "_sidou.wav", ours, 44100);
            write_wav16(dir + "/" + s.name + "_" + tag + "_residfp.wav", ref, 44100);
            double d = max_envelope_diff_db(normalized(rms_envelope_db(ours, 44100, 20), SIZE_MAX),
                                            normalized(rms_envelope_db(ref, 44100, 20), SIZE_MAX), -40, 3);
            std::printf("%-16s %-8s %.1f\n", s.name, tag, d);
        }
    }
    return 0;
}
```

**Step 6: Run everything**

Run: `cmake --build --preset host && ctest --preset host`
Expected: everything passes. A pre-check of this plan measured:
- **pitch:** 0.03 cents apart, against a tolerance of 0.1;
- **envelope:** 0.64 dB for the 8580 (tolerance 1.0) and 2.26 dB for the
  6581 (tolerance 2.5).

> **Superseded in code review.** The repository code under `tests/reference/`
> is the reference, not the blocks above. Code review found two measurement
> artefacts. First, 10 ms envelope windows aren't a whole number of periods,
> which gives ±0.5 dB phase ripple; they are now 4-period windows, normalised to
> the mean of the sustain plateau. Second, DECIMATE's true output rate is
> ~44100.80 Hz, not 44100 Hz. With both corrected, the code measured: pitch
> 0.0014 cents; envelope 0.55 dB (8580) and 2.05 dB (6581). It also found that
> the 8580's release is ~5.5% steeper than reSIDfp's, which is an M1 item.
> A follow-up aligned the start state: reSIDfp powers each envelope counter up
> at 0xaa, while ours starts at 0, so the harness releases reSIDfp's envelopes
> to 0 first. Envelopes are now compared from 9 ms on, including the attack:
> 0.44 dB (8580, tolerance tightened to 0.75) and 2.04 dB (6581).
> The `sidou_compare` figures in Step 6 are superseded too. Each curve used to
> be normalised to its loudest window, which was window 0 with the start-up
> click of the volume step, outside the compared range. Normalising over the
> compared windows fixes that and makes the table independent of the pre-roll:
> 6581 7.0 (LP), 5.2 (BP) and 2.0 (HP) dB; 8580 1.3 (LP), 1.5 (BP) and 2.8 (HP) dB.

The 6581's gap is reSIDfp's non-linear envelope DAC, which M0 doesn't model.
It's an M1 item ("6581 DAC curve").

Lessons from that pre-check, which are already built into the code above:
- Autocorrelation pitch estimates are biased on aliased sawtooths by about
  0.3 Hz, so the harness uses a Hann-windowed DFT peak instead.
- reSIDfp's `RESAMPLE` runs about 0.02% fast, so the pitch test uses its
  exact-rate `DECIMATE` mode.
- reSIDfp's 8580 has a start-up transient, so envelopes are normalised to the
  sustain phase rather than to the peak.

Run: `mkdir -p build/compare && ./build/host/tests/reference/sidou_compare build/compare`
Expected: a six-row table and twelve WAV files. The pre-check gave:
- 6581: 6.9 (LP), 9.8 (BP) and 2.1 (HP) dB;
- 8580: 1.3 (LP), 1.7 (BP) and 5.6 (HP) dB.

These are the filter gaps M1 closes. Listen to a few pairs; the numbers go
into the decision record.

**Step 7: Commit**

```bash
git add tests
git commit -m "test: reSIDfp reference harness, pitch/envelope checks, compare tool"
```

---

### Task 11: Pinned pi32v2 toolchain in Docker

**Files:**
- Create: `tools/docker/Dockerfile`, `tools/docker/toolchain.sha256`
- Create: `tools/fm1-docker`
- Create: `cmake/pi32v2.cmake`
- Modify: `REUSE.toml`

**Step 1: Record the toolchain's hash**

JieLi's download URL isn't versioned. Pinning its SHA-256 means a silent
upstream change breaks the build loudly instead of changing the output.

Run:
```bash
curl -fL --retry 3 -o /tmp/jieli-toolchain.tar.xz https://pkgman.jieliapp.com/s/linux-toolchain
shasum -a 256 /tmp/jieli-toolchain.tar.xz | cut -d' ' -f1 > tools/docker/toolchain.sha256
tar -tJf /tmp/jieli-toolchain.tar.xz | head -3
```
Expected: one hex line in `toolchain.sha256`. The listing shows a
`jieli-linux-toolchains-*` top-level directory.

**Step 2: Write the Dockerfile**

`tools/docker/Dockerfile`:
```dockerfile
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
```

**Step 3: Write the wrapper**

`tools/fm1-docker` (make it executable with `chmod +x`):
```sh
#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Run a command in the pinned pi32v2 build image, e.g.  tools/fm1-docker cmake --preset fm1
# The repo is mounted at the same path, so CMake caches and paths stay valid outside.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
docker build --quiet --platform linux/amd64 -t sidou-pi32v2 \
    --build-arg TOOLCHAIN_SHA256="$(cat "$root/tools/docker/toolchain.sha256")" \
    "$root/tools/docker" >/dev/null
exec docker run --rm --platform linux/amd64 -u "$(id -u):$(id -g)" \
    -v "$root:$root" -w "$root" sidou-pi32v2 "$@"
```

**Step 4: Write the CMake toolchain file**

`cmake/pi32v2.cmake`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# JieLi pi32v2 (AC791N) bare-metal toolchain: clang 4.0.1 + a separate ld.
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR pi32v2)

set(JIELI_TOOLCHAIN "$ENV{JIELI_TOOLCHAIN}" CACHE PATH "JieLi toolchain root")
if(NOT EXISTS "${JIELI_TOOLCHAIN}/pi32v2/bin/clang")
    message(FATAL_ERROR "No pi32v2 clang under JIELI_TOOLCHAIN='${JIELI_TOOLCHAIN}'. "
                        "Build through tools/fm1-docker.")
endif()

set(CMAKE_C_COMPILER "${JIELI_TOOLCHAIN}/pi32v2/bin/clang")
set(CMAKE_ASM_COMPILER "${JIELI_TOOLCHAIN}/pi32v2/bin/clang")
set(CMAKE_C_COMPILER_TARGET pi32v2)
set(CMAKE_ASM_COMPILER_TARGET pi32v2)
set(CMAKE_LINKER "${JIELI_TOOLCHAIN}/pi32v2/bin/ld")
set(CMAKE_OBJCOPY "${JIELI_TOOLCHAIN}/common/bin/objcopy")
set(CMAKE_OBJDUMP "${JIELI_TOOLCHAIN}/common/bin/objdump")
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

set(CMAKE_C_FLAGS_INIT "-ffreestanding -fno-builtin")
set(CMAKE_C_FLAGS_RELEASE_INIT "-O2")
set(CMAKE_C_LINK_EXECUTABLE "<CMAKE_LINKER> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")
```

**Step 5: Check that CMake configures for the target**

Run: `tools/fm1-docker cmake --preset fm1`
Expected: `-- Configuring done`. Only `core/` and `bench/` take part (tests are
host-only). If compiler detection fails on clang 4.0.1, set
`CMAKE_C_COMPILER_FORCED ON` and `CMAKE_ASM_COMPILER_FORCED ON` in the
toolchain file. Write down which one you needed in the commit message.

Run: `tools/fm1-docker cmake --build --preset fm1`
Expected: `libsidou_core.a` and `libsidou_workload.a` build with no errors.

**Step 6: Add `tools/docker/toolchain.sha256` to `REUSE.toml`, then commit**

```bash
git add tools cmake REUSE.toml
git commit -m "build: pinned JieLi pi32v2 toolchain in Docker with CMake toolchain file"
```

---

### Task 12: Bare-metal pi32v2 benchmark program

**Files:**
- Create: `bench/fm1/CMakeLists.txt`, `bench/fm1/crt0.S`, `bench/fm1/bench.ld`
- Create: `bench/fm1/bench_main.c`, `bench/fm1/libc_min.c`
- Create: `LICENSES/BSD-2-Clause.txt`
- Modify: `bench/CMakeLists.txt` (the `CMAKE_CROSSCOMPILING` branch from Task 9)

`crt0.S` and `bench.ld` are adapted from fm1-sid (`fm1-emu/fm1-sid/fm1/`,
BSD-2-Clause, © 2026 Joakim Eriksson). They keep that licence and copyright,
with ours added alongside.

**Step 1: Fetch the BSD-2-Clause text**

Run: `curl -sSfL https://spdx.org/licenses/BSD-2-Clause.txt -o LICENSES/BSD-2-Clause.txt`
Then trim it to the licence text if SPDX's page wraps it in anything else.
`reuse download BSD-2-Clause` does the same thing more cleanly; use that if
`reuse` is installed.

**Step 2: Startup code and linker script**

`bench/fm1/crt0.S`:
```asm
/* SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: BSD-2-Clause
 * Bare-metal pi32v2 entry (after fm1-sid's fm1/crt0.S): set both stacks, call main. */
	.section .text.crt0,"ax",@progbits
	.globl _start
_start:
	goto 1f
	rts
1:	cli
	sp = 0x01C7A000
	ssp = 0x01C7C000
	call main
	goto .
```

`bench/fm1/bench.ld`:
```ld
/* SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: BSD-2-Clause
 * Code in the XIP window; data/bss in the SRAM Felucca uses for RAM + POOL. */
MEMORY
{
    XIP (rx)  : ORIGIN = 0x02000000, LENGTH = 0x80000
    RAM (rwx) : ORIGIN = 0x01C08000, LENGTH = 0x6C000
}
SECTIONS
{
    .text : { KEEP(*(.text.crt0)) *(.text*) *(.rodata*) . = ALIGN(4); } > XIP
    .data : { . = ALIGN(4); __data_start = .; *(.data*) *(.sdata*) . = ALIGN(4); __data_end = .; } > RAM AT > XIP
    __data_load = LOADADDR(.data);
    .bss (NOLOAD) : { . = ALIGN(4); __bss_start = .; *(.bss*) *(.sbss*) *(COMMON) . = ALIGN(4); __bss_end = .; } > RAM
    /DISCARD/ : { *(.comment) *(.note*) }
}
```

**Step 3: The benchmark program**

`bench/fm1/bench_main.c`:
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Runs the shared workload between bench_mark() calls. The emulator test counts
 * instructions between the marks and reads the samples from RESULT_ADDR. */
#include <stdint.h>

#include "workload.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

int main(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    int16_t* samples = (int16_t*)(RESULT_ADDR + 0x40u);

    bench_mark();
    workload_begin(BENCH_CHIPS, SID_MODEL_6581);
    bench_mark();
    workload_render_samples(samples, WORKLOAD_SAMPLES);
    bench_mark();

    result[1] = WORKLOAD_SAMPLES;
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
```

`bench/fm1/libc_min.c` (the compiler can emit calls to these for struct
copies and zeroing; there's no libc on the target):
```c
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stddef.h>

void* memset(void* dst, int c, size_t n) {
    unsigned char* d = dst;
    while (n--) {
        *d++ = (unsigned char)c;
    }
    return dst;
}

void* memcpy(void* dst, const void* src, size_t n) {
    unsigned char* d = dst;
    const unsigned char* s = src;
    while (n--) {
        *d++ = *s++;
    }
    return dst;
}
```

`bench/fm1/CMakeLists.txt`:
```cmake
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
enable_language(ASM)
foreach(chips 1 2)
    set(t sid_bench_${chips})
    add_executable(${t} crt0.S bench_main.c libc_min.c)
    set_target_properties(${t} PROPERTIES SUFFIX ".elf")
    target_compile_definitions(${t} PRIVATE BENCH_CHIPS=${chips})
    target_link_libraries(${t} PRIVATE sidou_workload)
    target_link_options(${t} PRIVATE -T ${CMAKE_CURRENT_SOURCE_DIR}/bench.ld -e _start)
    # objcopy -O binary places sections at their load addresses: .data lands right after .text.
    add_custom_command(TARGET ${t} POST_BUILD
        COMMAND ${CMAKE_OBJCOPY} -O binary -j .text -j .data $<TARGET_FILE:${t}> ${t}.bin
        COMMAND ${CMAKE_OBJDUMP} -d $<TARGET_FILE:${t}> > ${t}.dis
        WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})
endforeach()
```

**Step 4: Build and inspect it**

Run: `tools/fm1-docker cmake --build --preset fm1`
Expected: `build/fm1/bench/fm1/sid_bench_{1,2}.{elf,bin,dis}`.

Run: `grep -cE "call.*__(add|sub|mul|div)sf3" build/fm1/bench/fm1/sid_bench_1.dis || true`
Expected: `0`. Any other number means float is going through software
routines instead of the FPU. Stop here and decide, and write the decision
down in the decision record:
- find the clang flag that turns on the hardware FPU (check what Felucca
  passes and how its `phys_dsp.c` floats compile), **or**
- switch the filter to fixed-point.

If the link fails with undefined `__*sf3`, `__udivsi3` or `__umodsi3`, find
the compiler runtime library in the toolchain
(`find $JIELI_TOOLCHAIN -name "*.a" | grep -i -E "rt|builtins"`) and add it to
`target_link_libraries`.

**Step 5: Commit**

```bash
git add bench LICENSES/BSD-2-Clause.txt
git commit -m "bench: bare-metal pi32v2 SID benchmark program"
```

---

### Task 13: Emulator benchmark test in Rust (the M0 decision gate)

This crate uses fm1-emu as a library. That means no parsing of emulator text
output: the test steps the CPU itself, counts instructions between
`bench_mark` calls (found through the ELF symbol table), and compares the
target's samples with the same C code built for the host. Rust is justified
here because fm1-emu is Rust and this is the only way to drive it without
scraping text.

**Files:**
- Create: `tests/target/Cargo.toml`, `tests/target/build.rs`
- Create: `tests/target/src/lib.rs`, `tests/target/tests/sid_budget.rs`
- Modify: `REUSE.toml` (add `tests/target/Cargo.lock`)

**Step 1: Crate setup**

`tests/target/Cargo.toml`:
```toml
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
[package]
name = "sidou-target-tests"
version = "0.0.0"
edition = "2021"
publish = false
license = "GPL-3.0-or-later"

[dependencies]

[dev-dependencies]
fm1-core = { git = "https://github.com/joakimeriksson/fm1-emu", rev = "e52f4640157f490f8c3af778dd044e9d33e989e8" }
fm1-soc = { git = "https://github.com/joakimeriksson/fm1-emu", rev = "e52f4640157f490f8c3af778dd044e9d33e989e8" }
object = { version = "0.36", default-features = false, features = ["read", "elf", "std"] }
serde_json = "1"

[build-dependencies]
cc = "1"

[profile.test]
opt-level = 3
```

`tests/target/build.rs` (builds the same C sources for the host, for the
comparison):
```rust
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = ["core/sid/sid.c", "core/sid/sid_filter.c", "bench/workload.c"];
    for s in &sources {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }
    println!("cargo:rerun-if-changed={}", root.join("core/sid").display());
    cc::Build::new()
        .files(sources.iter().map(|s| root.join(s)))
        .include(root.join("core"))
        .include(root.join("bench"))
        .flag("-std=c11")
        .flag("-ffp-contract=off")
        .opt_level(2)
        .compile("sidou_host");
}
```

`tests/target/src/lib.rs`:
```rust
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host build of the SIDOU workload, for comparing against the pi32v2 build.

pub const SID_MODEL_6581: u32 = 0;

extern "C" {
    fn workload_begin(chips: u32, model: u32);
    fn workload_render_samples(out: *mut i16, n: u32);
    fn workload_sample_count() -> u32;
}

pub fn render_host(chips: u32) -> Vec<i16> {
    unsafe {
        let n = workload_sample_count();
        let mut out = vec![0i16; n as usize];
        workload_begin(chips, SID_MODEL_6581);
        workload_render_samples(out.as_mut_ptr(), n);
        out
    }
}
```

**Step 2: Write the test**

`tests/target/tests/sid_budget.rs`:
```rust
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs the pi32v2 SID benchmark in fm1-emu and enforces the M0 budget.
//! Instructions are not cycles: pi32v2 mostly issues one per cycle, but XIP loads
//! and divides take longer, so treat the result as a lower bound.

use fm1_core::Cpu;
use fm1_soc::{Soc, RAM_BASE, XIP_BASE};
use object::{Object, ObjectSymbol};

const RESULT_ADDR: u32 = 0x01C8_0000;
const RESULT_MAGIC: u32 = 0x5349_4442;
const CORE_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_100.0;
/// Design doc section 8: at most 900 instructions per sample per chip.
const BUDGET_PER_CHIP: f64 = 900.0;
const MAX_STEPS: u64 = 1_000_000_000;

struct Run {
    init: u64,
    render: u64,
    samples: Vec<i16>,
}

fn build_dir() -> String {
    std::env::var("SIDOU_FM1_BUILD")
        .expect("set SIDOU_FM1_BUILD to the fm1 build directory (e.g. $PWD/build/fm1)")
}

fn read_u32(soc: &Soc, addr: u32) -> u32 {
    let o = addr - RAM_BASE;
    u32::from_le_bytes([soc.ram.read8(o), soc.ram.read8(o + 1), soc.ram.read8(o + 2), soc.ram.read8(o + 3)])
}

fn run(chips: u32) -> Run {
    let dir = format!("{}/bench/fm1", build_dir());
    let elf = std::fs::read(format!("{dir}/sid_bench_{chips}.elf")).expect("bench ELF; build the fm1 preset");
    let bin = std::fs::read(format!("{dir}/sid_bench_{chips}.bin")).expect("bench image");
    let file = object::File::parse(&*elf).expect("valid ELF");
    let mark = file
        .symbols()
        .find(|s| s.name() == Ok("bench_mark"))
        .expect("bench_mark symbol")
        .address() as u32;

    let mut soc = Soc::new(1 << 20, bin);
    let mut cpu = Cpu::new(XIP_BASE);
    let mut marks = Vec::new();
    let mut steps: u64 = 0;
    while read_u32(&soc, RESULT_ADDR) != RESULT_MAGIC {
        if cpu.pc == mark {
            marks.push(steps);
        }
        cpu.step(&mut soc).unwrap_or_else(|e| panic!("cpu fault at {:#010x}: {e:?}", cpu.pc));
        steps += 1;
        assert!(steps < MAX_STEPS, "benchmark did not finish (pc {:#010x})", cpu.pc);
    }
    assert_eq!(marks.len(), 3, "expected 3 bench_mark calls, saw {}", marks.len());

    let n = read_u32(&soc, RESULT_ADDR + 4) as usize;
    let base = RESULT_ADDR + 0x40 - RAM_BASE;
    let samples = (0..n)
        .map(|i| {
            let o = base + 2 * i as u32;
            i16::from_le_bytes([soc.ram.read8(o), soc.ram.read8(o + 1)])
        })
        .collect();
    Run { init: marks[1] - marks[0], render: marks[2] - marks[1], samples }
}

fn check(chips: u32) {
    let r = run(chips);
    let host = sidou_target_tests::render_host(chips);
    assert_eq!(r.samples.len(), host.len());
    let worst = r
        .samples
        .iter()
        .zip(&host)
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap();

    let per_sample = r.render as f64 / r.samples.len() as f64;
    let pct_core = per_sample * SAMPLE_RATE / CORE_HZ * 100.0;
    let report = serde_json::json!({
        "chips": chips,
        "instructions_per_sample": per_sample,
        "percent_of_240mhz_core": pct_core,
        "init_instructions": r.init,
        "max_sample_diff_vs_host": worst,
    });
    println!("{report}");
    let out = format!("{}/sid_budget_{chips}.json", build_dir());
    std::fs::write(&out, report.to_string()).expect("write report");

    assert!(worst <= 1, "target output differs from host build by {worst} LSB");
    assert!(
        per_sample <= BUDGET_PER_CHIP * chips as f64,
        "{per_sample:.0} instructions/sample exceeds the {chips}-chip budget of {}",
        BUDGET_PER_CHIP * chips as f64
    );
}

#[test]
fn one_chip_fits_budget_and_matches_host() {
    check(1);
}

#[test]
fn two_chips_fit_budget_and_match_host() {
    check(2);
}
```

**Step 3: Run it**

Run:
```bash
SIDOU_FM1_BUILD=$PWD/build/fm1 cargo test --release \
  --manifest-path tests/target/Cargo.toml -- --nocapture --test-threads=1
```
Expected: two JSON lines with `instructions_per_sample`, then either PASS or
an over-budget failure. **Both are valid M0 results.** An over-budget failure
is the decision gate doing its job; go to Task 15.

Things that can go wrong, and what they mean:
- **`cpu fault`:** the emulator hit an instruction it doesn't model. The
  likeliest one is an FPU op. Report it upstream to fm1-emu with the PC and
  the disassembly line from `sid_bench_1.dis`.
- **`differs from host`:** float evaluation differs between the host and
  pi32v2. Check the `.dis` for fused multiply-adds, and check that
  `-ffp-contract=off` reached the target compile line.

**Step 4: Commit (including Cargo.lock, for reproducibility)**

```bash
git add tests/target REUSE.toml
git commit -m "test: fm1-emu benchmark gate for SID cost and host/target equivalence"
```

---

### Task 14: Continuous integration

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Write the workflow**

```yaml
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
name: ci
on:
  push:
    branches: [main]
  pull_request:
permissions:
  contents: read

jobs:
  host:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-24.04, macos-14]
        preset: [host, host-asan]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Install Ninja
        run: |
          if [ "$RUNNER_OS" = Linux ]; then sudo apt-get update && sudo apt-get install -y ninja-build; else brew install ninja; fi
      - run: cmake --preset ${{ matrix.preset }}
      - run: cmake --build --preset ${{ matrix.preset }}
      - run: ctest --preset ${{ matrix.preset }}

  reuse:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: fsfe/reuse-action@v5

  fm1:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: tools/fm1-docker cmake --preset fm1
      - run: tools/fm1-docker cmake --build --preset fm1
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: tests/target
      - name: Emulator benchmark gate
        env:
          SIDOU_FM1_BUILD: ${{ github.workspace }}/build/fm1
        run: cargo test --release --manifest-path tests/target/Cargo.toml -- --nocapture --test-threads=1
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: sid-budget
          path: build/fm1/sid_budget_*.json
```

**Step 2: Check the workflow locally as far as possible**

Run: `ctest --preset host && ctest --preset host-asan && reuse lint`
Expected: all green. If `actionlint` is installed, run it on the workflow
file.

**Step 3: Commit**

```bash
git add .github
git commit -m "ci: host tests on Linux/macOS with sanitizers, REUSE lint, fm1 emulator gate"
```

---

### Task 15: Documentation, licensing and the M0 decision record

**Files:**
- Create: `BUILDING.md`, `TESTING.md`
- Create: `docs/decisions/0001-test-and-benchmark-harness.md`
- Create: `docs/decisions/0002-m0-sid-cost.md`
- Modify: `LICENSING.md`, `CREDITS.md`, `CHANGELOG.md`, `README.md`

**Step 1: `BUILDING.md`**

What it needs:
- **Prerequisites:** CMake ≥ 3.25, Ninja, a C11/C++17 compiler, Docker (for
  the fm1 preset), and Rust stable (for the emulator gate).
- **The three presets,** with the exact commands from Tasks 1, 11 and 12.
- **How the toolchain is pinned:** `toolchain.sha256`, and what to do when
  the hash check fails. Download the new archive, check that it's a genuine
  JieLi release, update the hash in its own commit, and re-run the emulator
  gate.

**Step 2: `TESTING.md`**

It covers each test layer, the command that runs it and what a failure
means:
- unit tests;
- golden tests (`SIDOU_GOLDEN_UPDATE=1`, and review the audio before
  accepting a change);
- reSIDfp reference tests and `sidou_compare`;
- host benchmark;
- emulator gate.

It also states the rule from Task 10: tolerances only get tighter, unless a
decision record says otherwise.

**Step 3: Decision record 0001: test and benchmark harness**

Write it in the usual decision-record format: Context, Decision,
Consequences. It records:
- **The choice:** GoogleTest, Google Benchmark, CTest and CMake presets,
  instead of ad-hoc scripts.
- **Rust only for the emulator gate,** because fm1-emu is a Rust library,
  and using it directly avoids parsing text output.
- **reSIDfp as a test-only reference.**
- **No Python in M0.**

**Step 4: Decision record 0002: M0 SID cost**

Fill it in with the real numbers from Tasks 9, 10 and 13:
- instructions per sample for one and two chips;
- percentage of a 240 MHz core;
- init cost;
- the host benchmark result;
- the reference-test margins and the `sidou_compare` table;
- whether the FPU is used in hardware.

The decision is one of:
- **Go.** Within budget; M1 continues with this core.
- **Go with changes.** Over budget. Name the cut, in the design doc's order:
  chip B's oversampling, chip B's character, then the post-chip effects.
  Or name the optimisation (fixed-point filter, table-driven envelope).
- **No-go.** Explain why, and what replaces this core.

**Step 5: Update `LICENSING.md`'s component table**

| Component | Origin | Licence | Use | Status |
|---|---|---|---|---|
| reSID 0.16 filter cutoff points (`core/sid/sid_curves_resid016.h`) | Dag Lem, reSID 0.16 | GPL-2.0-or-later | data compiled into firmware | M0 stand-in, replaced in M1 |
| `bench/fm1/crt0.S`, `bench/fm1/bench.ld` | fm1-sid (Joakim Eriksson) | BSD-2-Clause | benchmark program only | in use |
| libresidfp @ `c1791383` | libsidplayfp project | GPL-2.0-or-later | host tests only, fetched at build time | in use |
| GoogleTest v1.15.2 | Google | BSD-3-Clause | host tests only, fetched | in use |
| Google Benchmark v1.9.1 | Google | Apache-2.0 | host benchmark only, fetched | in use |
| fm1-emu (`fm1-core`, `fm1-soc`) @ `e52f4640` | Joakim Eriksson | MIT | emulator gate only, fetched | in use |
| JieLi pi32v2 toolchain | Zhuhai JieLi Technology | proprietary, freely downloadable | build tool, not distributed | in use |

Also add to `CREDITS.md`:
- Andrew Simper (Cytomic), for the SVF design;
- Klaus Dormann and Tom Harte, for the 6502 test suites coming in M3.

**Step 6: `CHANGELOG.md` and `README.md`**

- `CHANGELOG.md`: under `[Unreleased] / Added`, list the SID core, the test
  harness and the emulator benchmark gate.
- `README.md`: update the status line to "M0: SID core and benchmark harness".
  Link `BUILDING.md` and `TESTING.md`.

**Step 7: Verify and commit**

Run: `reuse lint && ctest --preset host`
Expected: compliant, and all tests pass.

```bash
git add -A
git commit -m "docs: building and testing guides, harness and M0 cost decision records"
```

---

## Done when

- `ctest --preset host` and `ctest --preset host-asan` pass on macOS and
  Linux.
- `reuse lint` passes.
- The emulator gate runs in CI and publishes `sid_budget_*.json`.
- `docs/decisions/0002-m0-sid-cost.md` records a go, go-with-changes or
  no-go decision, backed by the measured numbers.

<!-- REUSE-IgnoreEnd -->
