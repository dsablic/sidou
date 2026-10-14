# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
include(FetchContent)
FetchContent_Declare(googletest
    GIT_REPOSITORY https://github.com/google/googletest.git
    GIT_TAG v1.15.2
    GIT_SHALLOW TRUE)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
set(BUILD_GMOCK OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googletest)
include(GoogleTest)

add_executable(sidou_unit_tests
    unit/sid_defs_test.cpp
    unit/sid_osc_test.cpp)
target_link_libraries(sidou_unit_tests PRIVATE sidou_core GTest::gtest_main)
target_compile_options(sidou_unit_tests PRIVATE -Wall -Wextra)
gtest_discover_tests(sidou_unit_tests)
