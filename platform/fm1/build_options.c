// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifdef SIDOU_BUILD_OPTIONS
__attribute__((section(".sidou_options"), used)) const char sidou_build_options[] =
    SIDOU_BUILD_OPTIONS;
#endif
