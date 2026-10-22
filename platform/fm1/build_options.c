// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The build options that change the image, as text in the image: "SIDOU-OPTIONS 1 NAME=VALUE
// ..." (platform/fm1/CMakeLists.txt makes it, app.ld keeps it). `sidou-pack --verify` and
// tools/preflash read it from a package and fail one built with options off their defaults,
// as a stale or hand-changed CMake cache builds.
#ifdef SIDOU_BUILD_OPTIONS
__attribute__((section(".sidou_options"), used)) const char sidou_build_options[] =
    SIDOU_BUILD_OPTIONS;
#endif
