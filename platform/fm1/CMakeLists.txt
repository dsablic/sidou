# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
enable_language(ASM)

# The vendored Felucca HAL (GPL-3.0-only): header-only, plus two assembly files.
add_library(fm1_hal INTERFACE)
target_include_directories(fm1_hal INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/hal)

# Every HAL header and both .S files compile on their own; not linked into anything.
add_library(fm1_hal_check OBJECT hal_check.c hal/fm1_isr.S hal/fm1_vec.S)
target_link_libraries(fm1_hal_check PRIVATE fm1_hal)
# The headers define static helpers the check does not call.
target_compile_options(fm1_hal_check PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Werror -Wno-unused-function>)
