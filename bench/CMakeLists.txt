# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_workload STATIC workload.c)
target_include_directories(sidou_workload PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_workload PUBLIC sidou_core)
target_compile_options(sidou_workload PRIVATE -Wall -Wextra -Wshadow)

# The effects scenario (workload_fx.h).
add_library(sidou_workload_fx STATIC workload_fx.c)
target_link_libraries(sidou_workload_fx PUBLIC sidou_workload)
target_compile_options(sidou_workload_fx PRIVATE -Wall -Wextra -Wshadow)

# The effect-slot scenarios (workload_slots.h).
add_library(sidou_workload_slots STATIC workload_slots.c)
target_link_libraries(sidou_workload_slots PUBLIC sidou_workload)
target_compile_options(sidou_workload_slots PRIVATE -Wall -Wextra -Wshadow)

if(CMAKE_CROSSCOMPILING AND EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/fm1)
    add_subdirectory(fm1)
endif()
