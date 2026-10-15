// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The credits the About cracktro shows (app/about.h), in pages. app/credits.c is generated
 * from CREDITS.md by host/crates/sidou-tests/src/credits.rs (the test tests/app/credits.rs,
 * with SIDOU_CREDITS_UPDATE=1): a page per section, or more when it does not fit, each line
 * wrapped to CREDITS_COLS characters in upper case and the screen font's ASCII (accented
 * letters lose their accents, other characters become '?'), names between names. A credit
 * longer than a page starts one and goes on under "<SECTION> (CONT.)"; nothing shares a page
 * with its tail. */
#ifndef SIDOU_CREDITS_H
#define SIDOU_CREDITS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Characters a line holds at most: columns 2..27, inside the frame and its margin. */
#define CREDITS_COLS 26u
/* Lines on a page at most, the heading aside. */
#define CREDITS_LINES 8u

typedef struct {
    const char* heading; /* the section */
    /* NULL after the last line. */
    const char* line[CREDITS_LINES];
    /* Bit i: line i names who is credited (the part before the colon), shown brighter. */
    uint8_t names;
} credits_page;

extern const credits_page credits_pages[];
extern const uint32_t credits_page_count;

#ifdef __cplusplus
}
#endif

#endif
