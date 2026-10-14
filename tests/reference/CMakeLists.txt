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
target_include_directories(residfp SYSTEM PUBLIC ${RFP} ${RFP_GEN})
# Upstream builds with -ffast-math -fno-unsafe-math-optimizations. We deliberately
# don't: the reference should follow strict IEEE semantics like our own core.
target_compile_options(residfp PRIVATE -w)
find_package(Threads REQUIRED)
target_link_libraries(residfp PUBLIC Threads::Threads)

add_library(sidou_reference STATIC residfp_engine.cpp render.cpp signal_metrics.cpp)
target_include_directories(sidou_reference PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_reference PUBLIC residfp sidou_core)
target_compile_options(sidou_reference PRIVATE -Wall -Wextra)

add_executable(sidou_reference_tests reference_test.cpp)
target_link_libraries(sidou_reference_tests PRIVATE sidou_reference GTest::gtest_main)
target_compile_options(sidou_reference_tests PRIVATE -Wall -Wextra)
gtest_discover_tests(sidou_reference_tests)

add_executable(sidou_compare sidou_compare.cpp)
target_link_libraries(sidou_compare PRIVATE sidou_reference)
target_compile_options(sidou_compare PRIVATE -Wall -Wextra)
# Smoke test: the compare tool runs and writes its WAV pairs.
file(MAKE_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/compare)
add_test(NAME sidou_compare_smoke COMMAND sidou_compare ${CMAKE_CURRENT_BINARY_DIR}/compare)
