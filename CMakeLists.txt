# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Firmware cross build only (preset fm1). Host tests, tools and benchmarks are the
# cargo workspace in host/.
cmake_minimum_required(VERSION 3.25)
project(sidou VERSION 0.0.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

add_subdirectory(core)
add_subdirectory(bench)
if(CMAKE_CROSSCOMPILING)
    add_subdirectory(platform/fm1)
endif()
