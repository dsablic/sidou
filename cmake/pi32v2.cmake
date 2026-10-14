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

# clang only searches its own resource headers; newlib's (string.h etc.) ship beside it.
set(CMAKE_C_STANDARD_INCLUDE_DIRECTORIES "${JIELI_TOOLCHAIN}/pi32v2/include")
# -mcpu=r3 turns on the single-precision FPU (the default CPU calls soft-float helpers);
# -mfprev1 lets the compiler dual-issue FPU ops (flags per fm1-emu's docs/isa-notes.md).
set(CMAKE_C_FLAGS_INIT "-mcpu=r3 -mfprev1 -ffreestanding -fno-builtin")
# A cache entry: CMake would otherwise append its Clang default -O3 to an _INIT value.
set(CMAKE_C_FLAGS_RELEASE "-O2 -DNDEBUG" CACHE STRING "C flags for Release builds")
set(CMAKE_C_LINK_EXECUTABLE "<CMAKE_LINKER> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")
