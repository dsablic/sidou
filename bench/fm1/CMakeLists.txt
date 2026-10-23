# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
enable_language(ASM)

# One bare-metal benchmark image: <name>.elf, <name>.bin and the disassembly <name>.dis.
function(sidou_bench name main chips)
    add_executable(${name} crt0.S ${main} libc_min.c)
    set_target_properties(${name} PROPERTIES SUFFIX ".elf")
    target_compile_definitions(${name} PRIVATE BENCH_CHIPS=${chips})
    target_link_libraries(${name} PRIVATE ${ARGN})
    target_link_options(${name} PRIVATE -T ${CMAKE_CURRENT_SOURCE_DIR}/bench.ld -e _start
        --orphan-handling=error)
    # JieLi's objcopy takes one -j per run. bench.ld loads .data right after .text, and
    # main() copies it from there, so the image is the two concatenated.
    add_custom_command(TARGET ${name} POST_BUILD
        COMMAND ${CMAKE_OBJCOPY} -O binary -j .text $<TARGET_FILE:${name}> ${name}.text.bin
        COMMAND ${CMAKE_OBJCOPY} -O binary -j .data $<TARGET_FILE:${name}> ${name}.data.bin
        COMMAND ${CMAKE_COMMAND} -E cat ${name}.text.bin ${name}.data.bin > ${name}.bin
        COMMAND ${CMAKE_OBJDUMP} -d $<TARGET_FILE:${name}> > ${name}.dis
        WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})
endfunction()

foreach(chips 1 2)
    sidou_bench(sid_bench_${chips} bench_main.c ${chips} sidou_workload)
    sidou_bench(sid_bench_fx_${chips} bench_fx_main.c ${chips} sidou_workload_fx)
    sidou_bench(sid_bench_fx_worst_${chips} bench_fx_main.c ${chips} sidou_workload_fx)
    target_compile_definitions(sid_bench_fx_worst_${chips} PRIVATE BENCH_FX_WORST)
    sidou_bench(sid_bench_slots_${chips} bench_slots_main.c ${chips} sidou_workload_slots)
endforeach()
# The song player's audio-interrupt block (one build: one chip, no render).
sidou_bench(sid_bench_song bench_song_main.c 1 sidou_core)
# SIDOU's 6502 as the firmware builds it (CPU6502_SMALL, -Os; M3 Task 2).
sidou_bench(sid_bench_6502 bench_6502_main.c 1 sidou_core)
# The pixel scene (one build: it has no chips).
sidou_bench(sid_bench_scene bench_scene_main.c 1 sidou_workload_scene)
