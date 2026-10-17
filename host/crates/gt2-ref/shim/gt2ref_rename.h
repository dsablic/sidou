/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* Included first in every GoatTracker file the harness builds (build.rs passes it with
 * -include): the C library's headers, then the renames of what the shim stands in for.
 * The renames come after the headers, not as -D on the command line: a C library may
 * wrap a function in its own header (glibc's _FORTIFY_SOURCE, which Ubuntu's compilers
 * turn on by default, makes fread an inline function that calls the real fread), and a
 * name renamed before the header is read renames that wrapper too, so GoatTracker's calls
 * reach the C library instead of the shim. Here the headers are read first (their include
 * guards keep GoatTracker's own includes from reading them again), so every call the
 * GoatTracker files make is renamed and goes to the shim. Written for SIDOU. */
#ifndef GT2REF_RENAME_H
#define GT2REF_RENAME_H

#include <stdio.h>
#include <stdlib.h>

#include "gt2ref_shim.h"

#define fopen gt2ref_fopen
#define fread gt2ref_fread
#define exit gt2ref_exit
#define main gt2reloc_main

#endif
