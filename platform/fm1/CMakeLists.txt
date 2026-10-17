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
add_executable(${t} crt0.S main.c audio.c input.c lcd.c midi.c store_fm1.c ui.c libc_min.c hal/fm1_isr.S
    hal/fm1_vec.S)
set_target_properties(${t} PROPERTIES SUFFIX ".elf")
target_link_libraries(${t} PRIVATE fm1_hal sidou_live sidou_core)
target_include_directories(${t} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
# The identity the update protocol answers and the package carries (sidou-pack --product):
# FM-1_900 for development builds, as every community firmware's; releases FM-1_9XY.
set(SIDOU_PRODUCT "FM-1_900" CACHE STRING "Update-protocol identity, FM-1_<digits>")
# Hardware-checklist builds only (docs/hardware-test.md), never released: with OCT- held at
# power-on the firmware dies SIDOU_TEST_DEATH_MS after boot, by a hang ("hang": the watchdog
# resets) or a P33 reboot ("reboot"), to check the boot guard, .noinit across resets and the
# watchdog in UBOOT. Without the key it runs as the normal build.
set(SIDOU_TEST_DEATH "" CACHE STRING "Checklist builds: hang or reboot early; empty: off")
set(SIDOU_TEST_DEATH_MS "1000" CACHE STRING "When SIDOU_TEST_DEATH strikes, ms after boot")
# MIDI out over USB (docs/reference/midi.md, "Output"), until M2 stores settings: the
# channel, and NRPNs at full resolution for the cutoff, pulse width and volume instead of CCs.
set(SIDOU_MIDI_OUT_CHANNEL "1" CACHE STRING "MIDI out channel, 1..16")
option(SIDOU_MIDI_OUT_NRPN "MIDI out: 14-bit NRPNs for the high-resolution parameters" OFF)
target_compile_definitions(${t} PRIVATE SIDOU_VERSION="${PROJECT_VERSION}"
    SIDOU_PRODUCT="${SIDOU_PRODUCT}" SIDOU_MIDI_OUT_CHANNEL=${SIDOU_MIDI_OUT_CHANNEL}
    SIDOU_MIDI_OUT_NRPN=$<BOOL:${SIDOU_MIDI_OUT_NRPN}>)
if(SIDOU_TEST_DEATH STREQUAL "hang" OR SIDOU_TEST_DEATH STREQUAL "reboot")
    message(WARNING "SIDOU_TEST_DEATH=${SIDOU_TEST_DEATH}: a checklist build that dies early")
    string(COMPARE EQUAL "${SIDOU_TEST_DEATH}" "reboot" death_reboot)
    target_compile_definitions(${t} PRIVATE SIDOU_TEST_DEATH_MS=${SIDOU_TEST_DEATH_MS}u
        SIDOU_TEST_DEATH_REBOOT=$<BOOL:${death_reboot}>)
elseif(NOT SIDOU_TEST_DEATH STREQUAL "")
    message(FATAL_ERROR "SIDOU_TEST_DEATH is hang, reboot or empty")
endif()
# The flash timing spike (M2a Task 3b, docs/hardware-test.md section 12), never released: a
# test firmware that measures the flash part's erase and program times on the store's last two
# sectors instead of running SIDOU. Off by default; the default image has none of it
# (emu-gate tests/flash_spike.rs checks its symbols).
option(SIDOU_FLASH_SPIKE "Build the flash timing spike instead of SIDOU (never released)" OFF)
if(SIDOU_FLASH_SPIKE)
    message(WARNING "SIDOU_FLASH_SPIKE=ON: the flash timing spike, a test firmware, not SIDOU")
    target_sources(${t} PRIVATE flash_spike.c)
    target_compile_definitions(${t} PRIVATE SIDOU_FLASH_SPIKE=1)
endif()
target_compile_options(${t} PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Wextra -Wshadow>)
# --gc-sections drops functions nothing calls (e.g. live_sid_sink, which only the host uses):
# besides the size, an unused call between flash and SRAM code would otherwise have to link.
# --emit-relocs keeps the relocations in the ELF (not in the image) for the emulator test
# that no branch crosses between flash and SRAM (emu-gate tests/ram_audio.rs).
target_link_options(${t} PRIVATE -T ${CMAKE_CURRENT_SOURCE_DIR}/app.ld -e _start
    --orphan-handling=error --gc-sections --emit-relocs)
set_property(TARGET ${t} APPEND PROPERTY LINK_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/app.ld)
# JieLi's objcopy takes one -j per run. app.ld loads crt0 (.entry), .ram_text, .text and
# .data back to back from XIP's origin, and fm1_cstart copies .ram_text and .data from there,
# so the image is the four concatenated.
add_custom_command(TARGET ${t} POST_BUILD
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .entry $<TARGET_FILE:${t}> ${t}.entry.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .ram_text $<TARGET_FILE:${t}> ${t}.ram_text.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .text $<TARGET_FILE:${t}> ${t}.text.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .data $<TARGET_FILE:${t}> ${t}.data.bin
    COMMAND ${CMAKE_COMMAND} -E cat ${t}.entry.bin ${t}.ram_text.bin ${t}.text.bin ${t}.data.bin
        > ${t}.bin
    COMMAND ${CMAKE_OBJDUMP} -d $<TARGET_FILE:${t}> > ${t}.dis
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})

# The update loader: Felucca's (loader/ and the src/ files it includes, GPL-3.0-only),
# unmodified, built with exactly the commands of Felucca's tools/build.py build_loader():
# once step 1 commits, the SPL runs this loader at every power-on until an install completes,
# so it must be the bytes proven on hardware (VENDORED.md). Not through our toolchain flags:
# no -mcpu, -O2 or -std, as build.py. loader/loader.bin is the RAM image for 0x01C0A800;
# sidou-pack wraps it into the package's ota.bin.
set(ldr ${CMAKE_CURRENT_BINARY_DIR}/loader)
set(ldr_src ${CMAKE_CURRENT_SOURCE_DIR}/loader)
set(ldr_flags -Os -ffunction-sections -fno-builtin -Wall -Wno-unused-function
    -I${CMAKE_CURRENT_SOURCE_DIR}/hal -I${CMAKE_CURRENT_SOURCE_DIR}/src)
add_custom_command(
    OUTPUT ${ldr}/loader.bin ${ldr}/loader.elf ${ldr}/loader.dis
    COMMAND ${CMAKE_COMMAND} -E make_directory ${ldr}
    COMMAND ${CMAKE_C_COMPILER} -target pi32v2 -c ${ldr_src}/crt0_ldr.S -o ${ldr}/crt0_ldr.o
    COMMAND ${CMAKE_C_COMPILER} -target pi32v2 ${ldr_flags} -c ${ldr_src}/loader.c
        -o ${ldr}/loader.o
    COMMAND ${CMAKE_LINKER} --gc-sections -e _start -T ${ldr_src}/loader.ld ${ldr}/crt0_ldr.o
        ${ldr}/loader.o -o ${ldr}/loader.elf
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .text ${ldr}/loader.elf ${ldr}/loader.bin
    COMMAND ${CMAKE_OBJDUMP} -d ${ldr}/loader.elf > ${ldr}/loader.dis
    DEPENDS ${ldr_src}/crt0_ldr.S ${ldr_src}/loader.c ${ldr_src}/ldr_core.c ${ldr_src}/loader.ld
        ${CMAKE_CURRENT_SOURCE_DIR}/src/usb.c ${CMAKE_CURRENT_SOURCE_DIR}/src/ota.c
        ${CMAKE_CURRENT_SOURCE_DIR}/src/libc.c
    VERBATIM)
add_custom_target(felucca_loader ALL DEPENDS ${ldr}/loader.bin)
