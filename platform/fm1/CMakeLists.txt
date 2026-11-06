# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
enable_language(ASM)

set(bsp ${CMAKE_CURRENT_SOURCE_DIR}/bsp)
add_library(fm1_hal INTERFACE)
target_include_directories(fm1_hal SYSTEM INTERFACE ${bsp}/chip ${bsp}/boards/fm1)

add_library(fm1_hal_check OBJECT hal_check.c bsp/chip/fm1_isr.S bsp/chip/fm1_vec.S)
target_link_libraries(fm1_hal_check PRIVATE fm1_hal)
target_compile_options(fm1_hal_check PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Werror -Wno-unused-function>)

set(t sidou_app)
add_executable(${t} crt0.S main.c audio.c input.c lcd.c midi.c store_fm1.c ui.c libc_min.c build_options.c
    bsp/chip/fm1_isr.S
    bsp/chip/fm1_vec.S)
set_target_properties(${t} PROPERTIES SUFFIX ".elf")
target_link_libraries(${t} PRIVATE fm1_hal sidou_live sidou_core)
target_include_directories(${t} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
set(SIDOU_PRODUCT "FM-1_900" CACHE STRING "Update-protocol identity, FM-1_<digits>")
set(SIDOU_TEST_DEATH "" CACHE STRING "Checklist builds: hang or reboot early; empty: off")
set(SIDOU_TEST_DEATH_MS "1000" CACHE STRING "When SIDOU_TEST_DEATH strikes, ms after boot")
set(SIDOU_MIDI_OUT_CHANNEL "1" CACHE STRING "MIDI out channel, 1..16")
option(SIDOU_MIDI_OUT_NRPN "MIDI out: 14-bit NRPNs for the high-resolution parameters" OFF)
set(SIDOU_SYSEX_GAP_MS "250" CACHE STRING "SysEx upload: ms between holds while the keys sound")
target_compile_definitions(${t} PRIVATE SIDOU_VERSION="${PROJECT_VERSION}"
    SIDOU_VERSION_MAJOR=${PROJECT_VERSION_MAJOR} SIDOU_VERSION_MINOR=${PROJECT_VERSION_MINOR}
    SIDOU_VERSION_PATCH=${PROJECT_VERSION_PATCH}
    SIDOU_PRODUCT="${SIDOU_PRODUCT}" SIDOU_MIDI_OUT_CHANNEL=${SIDOU_MIDI_OUT_CHANNEL}
    SIDOU_MIDI_OUT_NRPN=$<BOOL:${SIDOU_MIDI_OUT_NRPN}>
    STORE_SYSEX_GAP_MS=${SIDOU_SYSEX_GAP_MS}u)
if(SIDOU_TEST_DEATH STREQUAL "hang" OR SIDOU_TEST_DEATH STREQUAL "reboot")
    message(WARNING "SIDOU_TEST_DEATH=${SIDOU_TEST_DEATH}: a checklist build that dies early")
    string(COMPARE EQUAL "${SIDOU_TEST_DEATH}" "reboot" death_reboot)
    target_compile_definitions(${t} PRIVATE SIDOU_TEST_DEATH_MS=${SIDOU_TEST_DEATH_MS}u
        SIDOU_TEST_DEATH_REBOOT=$<BOOL:${death_reboot}>)
elseif(NOT SIDOU_TEST_DEATH STREQUAL "")
    message(FATAL_ERROR "SIDOU_TEST_DEATH is hang, reboot or empty")
endif()
option(SIDOU_FLASH_SPIKE "Build the flash timing spike instead of SIDOU (never released)" OFF)
if(SIDOU_FLASH_SPIKE)
    message(WARNING "SIDOU_FLASH_SPIKE=ON: the flash timing spike, a test firmware, not SIDOU")
    target_sources(${t} PRIVATE flash_spike.c)
    set_property(SOURCE flash_spike.c APPEND PROPERTY COMPILE_OPTIONS -fno-data-sections)
    target_compile_definitions(${t} PRIVATE SIDOU_FLASH_SPIKE=1)
endif()
option(SIDOU_BLE "Bluetooth MIDI on Optimist's open stack (measurement build, never released)" OFF)
if(SIDOU_BLE)
    set(SIDOU_BLE_SOURCE "${PROJECT_SOURCE_DIR}/build/ext/fm1-base" CACHE PATH
        "fm1-base's tree (bt/ble) with felucca-bsp at bsp/ (tools/ble-fetch)")
    set(SIDOU_BLE_CENTRAL "3" CACHE STRING
        "Connecting out: 0 peripheral only, 3 without pairing, 1 with")
    set(ble_stack ${SIDOU_BLE_SOURCE}/bt/ble)
    set(ble_chip ${SIDOU_BLE_SOURCE}/bsp/chip)
    set(ble_radio_headers fm1_ble.h fm1_ble_rf.h fm1_rf_cal.h fm1_rf_regs.h)
    foreach(f IN LISTS ble_radio_headers ITEMS ble_rf_tables_v15.h)
        if(NOT EXISTS ${ble_chip}/${f})
            message(FATAL_ERROR "SIDOU_BLE: ${ble_chip}/${f} is missing: run tools/ble-fetch")
        endif()
    endforeach()
    if(NOT EXISTS ${ble_stack}/ble_stack.c)
        message(FATAL_ERROR "SIDOU_BLE: ${ble_stack}/ble_stack.c is missing: run tools/ble-fetch")
    endif()
    set(ble_radio_dir ${CMAKE_CURRENT_BINARY_DIR}/ble_radio)
    foreach(f IN LISTS ble_radio_headers)
        configure_file(${ble_chip}/${f} ${ble_radio_dir}/${f} COPYONLY)
    endforeach()
    configure_file(${ble_chip}/ble_rf_tables_v15.h ${ble_radio_dir}/ble_rf_tables.h COPYONLY)
    target_sources(${t} PRIVATE ble_stack_unit.c)
    set_property(SOURCE ble_stack_unit.c APPEND PROPERTY COMPILE_OPTIONS -Os)
    set_property(SOURCE main.c ble_stack_unit.c APPEND PROPERTY INCLUDE_DIRECTORIES
        ${ble_stack} ${ble_radio_dir})
    set_property(SOURCE main.c ble_stack_unit.c APPEND PROPERTY COMPILE_DEFINITIONS
        BLE_DIAG=0 BLE_CENTRAL=${SIDOU_BLE_CENTRAL} BLE_DEVICE_NAME="SIDOU")
    set_property(SOURCE main.c midi.c APPEND PROPERTY COMPILE_DEFINITIONS SIDOU_BLE=1)
    message(WARNING
        "SIDOU_BLE=ON: Bluetooth MIDI's measurement build (central ${SIDOU_BLE_CENTRAL}), not a release")
    file(READ ${CMAKE_CURRENT_SOURCE_DIR}/app.ld app_ld)
    string(REPLACE "LENGTH = 0x8DFBC" "LENGTH = 0x100000" app_ld_measure "${app_ld}")
    if(app_ld_measure STREQUAL app_ld)
        message(FATAL_ERROR "SIDOU_BLE: app.ld's XIP length is not 0x8DFBC any more")
    endif()
    file(WRITE ${ble_radio_dir}/app.ld "${app_ld_measure}")
    set(sidou_app_ld ${ble_radio_dir}/app.ld)
else()
    set(sidou_app_ld ${CMAKE_CURRENT_SOURCE_DIR}/app.ld)
endif()
target_compile_options(${t} PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Wextra -Wshadow>)
option(SIDOU_USB_FIX "midi.c (the USB stack) with Felucca's build flags" ON)
set_property(SOURCE midi.c APPEND PROPERTY COMPILE_OPTIONS -fdata-sections)
set_property(SOURCE main.c APPEND PROPERTY COMPILE_OPTIONS -Os -mllvm -inline-threshold=225)
if(SIDOU_USB_FIX)
    set_property(SOURCE midi.c APPEND PROPERTY COMPILE_OPTIONS -mcpu=generic -mno-fprev1 -Os)
else()
    message(WARNING "SIDOU_USB_FIX=OFF: midi.c with SIDOU's flags, USB-MIDI out dead on the FM-1")
endif()
set(sidou_option_defaults
    "SIDOU_ALL_COMBINED_PROFILES=ON" "SIDOU_ALL_COMBINED_PROFILES_8580=ON"
    "SIDOU_FLASH_SPIKE=OFF" "SIDOU_USB_FIX=ON" "SIDOU_MIDI_OUT_NRPN=OFF"
    "SIDOU_MIDI_OUT_CHANNEL=1" "SIDOU_SYSEX_GAP_MS=250" "SIDOU_TEST_DEATH="
    "SIDOU_TEST_DEATH_MS=1000"
    "SIDOU_PRODUCT=FM-1_900" "CMAKE_BUILD_TYPE=Release")
set(sidou_options "")
set(sidou_options_off "")
foreach(entry IN LISTS sidou_option_defaults)
    string(FIND "${entry}" "=" eq)
    string(SUBSTRING "${entry}" 0 ${eq} name)
    math(EXPR eq1 "${eq} + 1")
    string(SUBSTRING "${entry}" ${eq1} -1 default)
    set(value "${${name}}")
    if(default STREQUAL "ON" OR default STREQUAL "OFF")
        if(value)
            set(value ON)
        else()
            set(value OFF)
        endif()
    endif()
    if(name STREQUAL "SIDOU_TEST_DEATH_MS" AND SIDOU_TEST_DEATH STREQUAL "")
        set(value "${default}")
    endif()
    if(NOT value STREQUAL default AND NOT name STREQUAL "SIDOU_PRODUCT")
        string(APPEND sidou_options_off "\n    ${name}=${value} (default ${default})")
    endif()
    if(value STREQUAL "")
        set(value "-")
    endif()
    string(APPEND sidou_options " ${name}=${value}")
endforeach()
if(SIDOU_BLE)
    string(APPEND sidou_options " SIDOU_BLE=${SIDOU_BLE_CENTRAL}")
    string(APPEND sidou_options_off "\n    SIDOU_BLE=ON (default OFF), central ${SIDOU_BLE_CENTRAL}")
endif()
if(NOT sidou_options_off STREQUAL "")
    message(WARNING "\n"
        "************************************************************************\n"
        "THIS BUILD DIRECTORY IS NOT A DEFAULT SIDOU BUILD. Options off their defaults:"
        "${sidou_options_off}\n"
        "A package of it is not a release (sidou-pack --verify and tools/preflash fail it).\n"
        "If you did not choose these, the cache is stale: build from a fresh build directory\n"
        "(rm -rf ${CMAKE_BINARY_DIR}).\n"
        "************************************************************************")
endif()
target_compile_definitions(${t} PRIVATE SIDOU_BUILD_OPTIONS="SIDOU-OPTIONS 1${sidou_options}")
target_link_options(${t} PRIVATE -T ${sidou_app_ld} -e _start
    --orphan-handling=error --gc-sections --emit-relocs)
set_property(TARGET ${t} APPEND PROPERTY LINK_DEPENDS ${sidou_app_ld})
add_custom_command(TARGET ${t} POST_BUILD
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .entry $<TARGET_FILE:${t}> ${t}.entry.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .ram_text $<TARGET_FILE:${t}> ${t}.ram_text.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .text $<TARGET_FILE:${t}> ${t}.text.bin
    COMMAND ${CMAKE_OBJCOPY} -O binary -j .data $<TARGET_FILE:${t}> ${t}.data.bin
    COMMAND ${CMAKE_COMMAND} -E cat ${t}.entry.bin ${t}.ram_text.bin ${t}.text.bin ${t}.data.bin
        > ${t}.bin
    COMMAND ${CMAKE_OBJDUMP} -d $<TARGET_FILE:${t}> > ${t}.dis
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})

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
