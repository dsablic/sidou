# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
enable_language(ASM)
foreach(chips 1 2)
    set(t sid_bench_${chips})
    add_executable(${t} crt0.S bench_main.c libc_min.c)
    set_target_properties(${t} PROPERTIES SUFFIX ".elf")
    target_compile_definitions(${t} PRIVATE BENCH_CHIPS=${chips})
    target_link_libraries(${t} PRIVATE sidou_workload)
    target_link_options(${t} PRIVATE -T ${CMAKE_CURRENT_SOURCE_DIR}/bench.ld -e _start
        --orphan-handling=error)
    # JieLi's objcopy takes one -j per run. bench.ld loads .data right after .text, and
    # main() copies it from there, so the image is the two concatenated.
    add_custom_command(TARGET ${t} POST_BUILD
        COMMAND ${CMAKE_OBJCOPY} -O binary -j .text $<TARGET_FILE:${t}> ${t}.text.bin
        COMMAND ${CMAKE_OBJCOPY} -O binary -j .data $<TARGET_FILE:${t}> ${t}.data.bin
        COMMAND ${CMAKE_COMMAND} -E cat ${t}.text.bin ${t}.data.bin > ${t}.bin
        COMMAND ${CMAKE_OBJDUMP} -d $<TARGET_FILE:${t}> > ${t}.dis
        WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})
endforeach()
