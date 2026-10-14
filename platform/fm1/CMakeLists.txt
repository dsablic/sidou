# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
enable_language(ASM)

# The vendored Felucca HAL (GPL-3.0-only): header-only, plus two assembly files.
add_library(fm1_hal INTERFACE)
# SYSTEM: the headers' unused static helpers do not warn in the files that include them.
target_include_directories(fm1_hal SYSTEM INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/hal)

# Every HAL header and both .S files compile on their own; not linked into anything.
add_library(fm1_hal_check OBJECT hal_check.c hal/fm1_isr.S hal/fm1_vec.S)
# Not through fm1_hal: warnings in SYSTEM headers are hidden, and this check is about them.
target_include_directories(fm1_hal_check PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/hal)
# The headers define static helpers the check does not call.
target_compile_options(fm1_hal_check PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Werror -Wno-unused-function>)

# The firmware: sidou_app.elf, sidou_app.bin (what the SPL maps at XIP 0x02000120) and .dis.
set(t sidou_app)
add_executable(${t} crt0.S main.c audio.c input.c lcd.c midi.c ui.c libc_min.c hal/fm1_isr.S
    hal/fm1_vec.S)
set_target_properties(${t} PROPERTIES SUFFIX ".elf")
target_link_libraries(${t} PRIVATE fm1_hal sidou_live sidou_core)
target_include_directories(${t} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(${t} PRIVATE SIDOU_VERSION="${PROJECT_VERSION}")
target_compile_options(${t} PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Wextra -Wshadow>)
target_link_options(${t} PRIVATE -T ${CMAKE_CURRENT_SOURCE_DIR}/app.ld -e _start
    --orphan-handling=error)
set_property(TARGET ${t} APPEND PROPERTY LINK_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/app.ld)
# JieLi's objcopy takes one -j per run. app.ld loads .ram_text and .data right after .text,
# and fm1_cstart copies them from there, so the image is the three concatenated.
add_custom_command(TARGET ${t} POST_BUILD
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .text $<TARGET_FILE:${t}> ${t}.text.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .ram_text $<TARGET_FILE:${t}> ${t}.ram_text.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .data $<TARGET_FILE:${t}> ${t}.data.bin
    COMMAND ${CMAKE_COMMAND} -E cat ${t}.text.bin ${t}.ram_text.bin ${t}.data.bin > ${t}.bin
    COMMAND ${CMAKE_OBJDUMP} -d $<TARGET_FILE:${t}> > ${t}.dis
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})
