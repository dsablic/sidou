# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_workload STATIC workload.c)
target_include_directories(sidou_workload PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_workload PUBLIC sidou_core)
target_compile_options(sidou_workload PRIVATE -Wall -Wextra -Wshadow)

if(CMAKE_CROSSCOMPILING)
    if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/fm1)
        add_subdirectory(fm1)
    endif()
else()
    add_subdirectory(host)
endif()
