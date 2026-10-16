// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The flash timing spike (M2a Task 3b, flash_spike.h): a test firmware for the user to run on
 * a real FM-1 (docs/hardware-test.md, section 12), never released, built only with
 * -DSIDOU_FLASH_SPIKE=ON. The store's write windows and held erases (M2a Task 4) are sized
 * from what it measures; fm1-emu's flash is instantaneous, so there every time reads 0.
 *
 * Safe by construction:
 * - every erase and program goes through flash_spike_guard.h, which refuses anything outside
 *   the store's last two sectors [0xDE000, 0xE0000) and, for programs, the guard band
 *   0xF00..0xFFF of each sector; the fm1_flash.h driver in this file has the same bounds as
 *   its own FL_RANGE_OK (it never reaches the OTA area, unlike midi.c's);
 * - before writing, it reads both sectors: each must be blank or the spike's own (its mark at
 *   offset 0, the guard band erased), and 0xDE000's pattern blank or intact. Anything else
 *   and it writes nothing, says REFUSED, and waits for an install;
 * - it starts only on SAVE, so a unit on which it misbehaves still boots into a screen that
 *   accepts the normal installer; the audio never starts.
 *
 * What it measures, in this order (the results struct below, read by emu-gate
 * tests/flash_spike.rs, and the screen):
 * - the part: JEDEC id, SR1/SR2 and the configure register (its DP bit makes a page 512
 *   bytes), the SFDP header and the basic table's erase types;
 * - the CPU clock: dependent adds a microsecond, from SRAM (docs/hardware.md, "Clock");
 * - the round-trip pattern in 0xDE000: absent, intact or damaged since the last run;
 * - 4 KiB sector erase (0x20), 20 runs, each of a sector programmed full;
 * - page programs (0x02) of 256 bytes (14 a run, 280), 16 and 64 bytes (20 each);
 * - several programs into one page (64 of 4 bytes), zeros over zeros (3 times), one byte
 *   programmed 8 times clearing a bit each time, and a page programmed twice with different
 *   patterns (an ECC part would not read back the AND), each read back;
 * - page erase (0x81): whether the part erases the page alone, and 20 runs if it does;
 * - erase suspend and resume (0x75/0x7A) during a sector erase: the latency, SUS1, and
 *   whether a read of 0xDE000 while suspended returns its data;
 * - what masking interrupts for 1 ms (100 times) and for 35 ms (10 times) costs the TRS
 *   input while a sender keeps the jack busy: bytes taken and bytes lost;
 * - at the end 0xDF000 is erased back to 0xFF and 0xDE000 keeps the pattern. */
#include <stddef.h>
#include <stdint.h>

#include "fm1_sys.h"
#include "fm1_time.h"

#include "flash_spike_guard.h"
/* This file's copy of the RAM driver erases and programs only the spike's sectors. */
#define FL_RANGE_OK(off, n) FLASH_SPIKE_DRIVER_OK(off, n)
#include "fm1_flash.h"

#include "flash_spike.h"
#include "font.h"
#include "grid.h"
#include "input.h"
#include "lcd.h"
#include "midi.h"
#include "panel.h"

/* ---- the RAM side: XIP off, interrupts masked, nothing but SFRs, the stack and RAM ---- */

RAMINL void flash_spike_wait_us(uint32_t us) {
    uint32_t t0 = T4_CNT;
    while ((T4_CNT - t0) / 24u < us) {
    }
}

/* SR1 | SR2 << 8 | the configure register (0x15) << 16. */
static RAMFN uint32_t flash_spike_regs_ram(void) {
    fl_saved_t s;
    uint32_t v;
    if (fl_enter(&s)) {
        return 0xFFFFFFFFu;
    }
    v = fl_rdsr(0x05) | ((uint32_t)fl_rdsr(0x35) << 8) | ((uint32_t)fl_rdsr(0x15) << 16);
    fl_exit(&s);
    return v;
}

/* Read SFDP (0x5A): n bytes from SFDP address 0. */
static RAMFN int flash_spike_sfdp_ram(uint8_t* dst, uint32_t n) {
    fl_saved_t s;
    uint32_t i;
    if (fl_enter(&s)) {
        return -2;
    }
    fl_cs(0);
    fl_tx(0x5A);
    fl_addr(0);
    fl_tx(0xFF);
    for (i = 0; i < n; i++) {
        dst[i] = fl_rx();
    }
    fl_cs(1);
    fl_exit(&s);
    return 0;
}

/* Page erase (0x81). 1 if the part left the write enable latch set (it ignored the command),
 * after clearing it. */
static RAMFN int flash_spike_page_erase_ram(uint32_t off, uint32_t* took_us) {
    fl_saved_t s;
    int rc;
    if (!FLASH_SPIKE_ERASE_OK(off, FLASH_SPIKE_PAGE)) {
        return -1;
    }
    if (fl_enter(&s)) {
        return -2;
    }
    fl_wren();
    if (!(fl_rdsr(0x05) & 2u)) {
        rc = -3;
    } else {
        fl_cs(0);
        fl_tx(0x81);
        fl_addr(off);
        fl_cs(1);
        fl_inval(off & ~0x1FFu, 2u * FLASH_SPIKE_PAGE); /* a dual page, if DP is set */
        rc = fl_wait_ready(100000u, took_us);
        if (rc == 0 && (fl_rdsr(0x05) & 2u)) {
            fl_cs(0);
            fl_tx(0x04); /* WRDI */
            fl_cs(1);
            rc = 1;
        }
    }
    fl_exit(&s);
    return rc;
}

/* A sector erase suspended after after_us, a fast read while suspended, the resume. */
static RAMFN int flash_spike_suspend_ram(uint32_t erase_off, uint32_t after_us,
                                         uint32_t read_off, uint8_t* dst, uint32_t n,
                                         flash_spike_suspend_result* out) {
    fl_saved_t s;
    uint32_t i;
    int rc;
    if (!FLASH_SPIKE_ERASE_OK(erase_off, FLASH_SPIKE_SECTOR) || !FLASH_SPIKE_IN(read_off, n)) {
        return -1;
    }
    if (fl_enter(&s)) {
        return -2;
    }
    fl_wren();
    if (!(fl_rdsr(0x05) & 2u)) {
        fl_exit(&s);
        return -3;
    }
    fl_cs(0);
    fl_tx(0x20);
    fl_addr(erase_off);
    fl_cs(1);
    flash_spike_wait_us(after_us);
    fl_cs(0);
    fl_tx(0x75);
    fl_cs(1);
    rc = fl_wait_ready(100000u, &out->latency_us);
    out->status = fl_rdsr(0x05) | ((uint32_t)fl_rdsr(0x35) << 8);
    fl_cs(0);
    fl_tx(0x0B);
    fl_addr(read_off);
    fl_tx(0xFF);
    for (i = 0; i < n; i++) {
        dst[i] = fl_rx();
    }
    fl_cs(1);
    fl_cs(0);
    fl_tx(0x7A);
    fl_cs(1);
    if (rc == 0) {
        rc = fl_wait_ready(500000u, &out->rest_us);
    }
    fl_inval(erase_off, FLASH_SPIKE_SECTOR);
    fl_exit(&s);
    return rc;
}

/* n rounds of 8 dependent adds; TIMER4 ticks (24 MHz) taken. */
static RAMFN uint32_t flash_spike_adds_ram(uint32_t n) {
    uint32_t x = 0, i, t0 = T4_CNT;
    for (i = 0; i < n; i++) {
        __asm__ volatile("%0 = %0 + %1\n\t%0 = %0 + %1\n\t%0 = %0 + %1\n\t%0 = %0 + %1\n\t"
                         "%0 = %0 + %1\n\t%0 = %0 + %1\n\t%0 = %0 + %1\n\t%0 = %0 + %1"
                         : "+r"(x)
                         : "r"(i));
    }
    return T4_CNT - t0;
}

/* ---- the driver calls flash_spike_guard.h declares: interrupts masked around each ---- */

static int flash_spike_drv_erase4k(uint32_t off, uint32_t* took_us) {
    uint32_t f = irq_save();
    int rc;
    input_leds_dark();
    rc = FL_FAR(fl_erase4k_ram)(off, took_us);
    irq_restore(f);
    return rc;
}

static int flash_spike_drv_page_erase(uint32_t off, uint32_t* took_us) {
    uint32_t f = irq_save();
    int rc;
    input_leds_dark();
    rc = FL_FAR(flash_spike_page_erase_ram)(off, took_us);
    irq_restore(f);
    return rc;
}

static int flash_spike_drv_prog(uint32_t off, const uint8_t* src, uint32_t n,
                                uint32_t* took_us) {
    uint32_t f = irq_save();
    int rc = FL_FAR(fl_prog_ram)(off, src, n, took_us);
    irq_restore(f);
    return rc;
}

static int flash_spike_drv_suspend(uint32_t erase_off, uint32_t after_us, uint32_t read_off,
                                   uint8_t* dst, uint32_t n, flash_spike_suspend_result* out) {
    uint32_t f = irq_save();
    int rc;
    input_leds_dark();
    rc = FL_FAR(flash_spike_suspend_ram)(erase_off, after_us, read_off, dst, n, out);
    irq_restore(f);
    out->rc = rc;
    return rc;
}

/* ---- the results ---- */

#define SPIKE_BOOT 0u /* not yet at its start screen */
#define SPIKE_WAIT 1u
#define SPIKE_RUN 2u
#define SPIKE_DONE 3u
#define SPIKE_REFUSED 4u
#define SPIKE_FAILED 5u

/* How the page erase behaved. */
#define PE_IGNORED 0u /* nothing changed */
#define PE_PAGE 1u    /* the page erased, nothing else */
#define PE_WIDER 2u   /* the page erased and more (a 512-byte dual page: DP set) */
#define PE_ODD 3u     /* anything else: the page not erased whole, or other bytes changed */

/* Microseconds: runs, min, max, sum. */
typedef struct {
    uint32_t n, min, max, sum;
} spike_stat;

/* Read by emu-gate tests/flash_spike.rs through the ELF symbol, as uint32_t words in this
 * order (the word index in each comment). */
typedef struct {
    uint32_t state;            /* 0: SPIKE_* */
    uint32_t step;             /* 1: the step running, or the one that failed */
    int32_t rc;                /* 2: its driver's error */
    uint32_t refused;          /* 3: calls the guard refused (a bug if not 0) */
    uint32_t jedec;            /* 4 */
    uint32_t regs;             /* 5: SR1 | SR2 << 8 | CR << 16 */
    uint32_t adds_per_us;      /* 6 */
    uint32_t class_pattern;    /* 7: 0xDE000 at the start, FLASH_SPIKE_BLANK/OURS/FOREIGN */
    uint32_t class_measure;    /* 8: 0xDF000 at the start */
    uint32_t pattern_at_start; /* 9: FLASH_SPIKE_PATTERN_* */
    uint32_t store_used;       /* 10: store sectors not blank at the start, of 73 */
    spike_stat erase4k;        /* 11-14 */
    spike_stat prog16;         /* 15-18 */
    spike_stat prog64;         /* 19-22 */
    spike_stat prog256;        /* 23-26 */
    spike_stat page_erase;     /* 27-30 */
    uint32_t page_erase_works; /* 31: PE_* */
    spike_stat suspend;        /* 32-35: the suspend latency */
    uint32_t sus_seen;         /* 36: suspends that showed SUS1 */
    uint32_t sus_read_bad;     /* 37: bytes of 0xDE000 read wrong while suspended */
    uint32_t sus_unfinished;   /* 38: bytes not erased after the resumes */
    uint32_t verify_bad;       /* 39: bytes read back wrong after the timed programs */
    uint32_t partial_bad;      /* 40: after 64 programs of 4 bytes into one page */
    uint32_t rezero_bad;       /* 41: after zeros programmed over zeros, 3 times */
    uint32_t bitwalk_bad;      /* 42: one byte programmed 8 times, a bit cleared each time */
    uint32_t twice_bad;        /* 43: pages programmed twice: bytes not the AND of both */
    uint32_t trs[6];           /* 44-49: bytes, lost: open 1 s; 100 x 1 ms masked; 10 x 35 ms */
    uint32_t sfdp[4];          /* 50-53: the signature, the basic table's DW1, DW8, DW9 */
    uint32_t final_bad;        /* 54: bytes of 0xDF000 not blank at the end, +1 if 0xDE000's
                                  pattern is not intact */
} spike_results;
static spike_results flash_spike_res __attribute__((used));

/* The sector images: what was read, and what 0xDF000 should hold. */
static uint8_t rd[FLASH_SPIKE_SECTOR] __attribute__((section(".pool"), aligned(4)));
static uint8_t expect[FLASH_SPIKE_SECTOR] __attribute__((section(".pool"), aligned(4)));
static uint8_t data[FLASH_SPIKE_PAGE] __attribute__((section(".pool"), aligned(4)));
static uint8_t sfdp[128] __attribute__((section(".pool"), aligned(4)));
static grid flash_spike_screen __attribute__((section(".pool")));
static uint16_t palette[16];

#define M FLASH_SPIKE_MEASURE
#define P FLASH_SPIKE_PATTERN
#define RUNS 20u

/* ---- the screen: SIDOU's grid and font, drawn whole after every step ---- */

#define TXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
#define HEAD GRID_ATTR(GRID_WHITE, GRID_BLUE)
#define GOOD GRID_ATTR(GRID_LIGHT_GREEN, GRID_BLUE)
#define BAD GRID_ATTR(GRID_LIGHT_RED, GRID_BLUE)
#define NOTE GRID_ATTR(GRID_YELLOW, GRID_BLUE)

static void flash_spike_show(void) {
    grid_run run;
    while (grid_next_run(&flash_spike_screen, &run)) {
        grid_render_run(&flash_spike_screen, &run, palette, lcd_buffer());
        lcd_send(run.col * GRID_CELL_PX, run.row * GRID_CELL_PX, run.len * GRID_CELL_PX,
                 GRID_CELL_PX);
    }
    lcd_sync();
}

static void text(uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    grid_text(&flash_spike_screen, col, row, s, attr);
}

/* v in decimal, right-aligned in w cells. */
static void dec(uint32_t col, uint32_t row, uint32_t w, uint32_t v, uint8_t attr) {
    char b[11];
    uint32_t i = 10;
    b[10] = 0;
    do {
        b[--i] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v && i);
    while (10u - i < w && i) {
        b[--i] = ' ';
    }
    text(col, row, &b[i], attr);
}

static void hex(uint32_t col, uint32_t row, uint32_t digits, uint32_t v, uint8_t attr) {
    char b[9];
    uint32_t i;
    for (i = 0; i < digits && i < 8u; i++) {
        b[digits - 1u - i] = "0123456789ABCDEF"[(v >> (4u * i)) & 15u];
    }
    b[i] = 0;
    text(col, row, b, attr);
}

static const char* class_name(uint32_t c) {
    return c == FLASH_SPIKE_BLANK ? "BLANK" : c == FLASH_SPIKE_OURS ? "SPIKE'S" : "FOREIGN";
}

/* min, mean, max in three columns of 7 from column 9, or "-" before the first run. */
static void times(uint32_t row, const char* name, const spike_stat* s) {
    text(0, row, name, TXT);
    if (s->n == 0u) {
        text(15, row, "-", TXT);
        return;
    }
    dec(9, row, 7, s->min, TXT);
    dec(16, row, 7, s->sum / s->n, TXT);
    dec(23, row, 7, s->max, TXT);
}

static void ok_or_count(uint32_t col, uint32_t row, uint32_t bad) {
    if (bad == 0u) {
        text(col, row, "OK", GOOD);
    } else {
        dec(col, row, 1, bad, BAD);
        text(col + (bad > 9u ? 2u : 1u) + (bad > 99u ? 1u : 0u) + (bad > 999u ? 1u : 0u),
             row, " BAD", BAD);
    }
}

static void draw(void) {
    spike_results* r = &flash_spike_res;
    grid_fill(&flash_spike_screen, 0, 0, GRID_COLS, GRID_ROWS, ' ', TXT);
    text(0, 0, "SIDOU FLASH SPIKE  M2A TASK 3B", HEAD);
    text(0, 2, "JEDEC", TXT);
    hex(6, 2, 6, r->jedec, TXT);
    text(13, 2, "SR", TXT);
    hex(16, 2, 4, r->regs & 0xFFFFu, TXT);
    text(21, 2, "CR", TXT);
    hex(24, 2, 2, r->regs >> 16, TXT);
    text(0, 3, "SFDP", TXT);
    hex(5, 3, 8, r->sfdp[0], TXT);
    hex(14, 3, 8, r->sfdp[1], TXT);
    text(0, 4, "ERASE", TXT);
    hex(6, 4, 8, r->sfdp[2], TXT);
    hex(15, 4, 8, r->sfdp[3], TXT);
    text(0, 5, "CLOCK", TXT);
    dec(6, 5, 1, r->adds_per_us, TXT);
    text(10, 5, "ADDS/US", TXT);
    text(0, 6, "START DE:", TXT);
    text(9, 6, class_name(r->class_pattern), TXT);
    text(17, 6, "DF:", TXT);
    text(20, 6, class_name(r->class_measure), TXT);
    text(0, 7, "PATTERN AT START", TXT);
    text(17, 7,
         r->pattern_at_start == FLASH_SPIKE_PATTERN_INTACT   ? "INTACT"
         : r->pattern_at_start == FLASH_SPIKE_PATTERN_ABSENT ? "ABSENT"
                                                             : "DAMAGED",
         r->pattern_at_start == FLASH_SPIKE_PATTERN_DAMAGED ? BAD : TXT);
    text(0, 8, "STORE SECTORS USED", TXT);
    dec(19, 8, 2, r->store_used, TXT);
    text(21, 8, "/73", TXT);
    if (r->state == SPIKE_WAIT) {
        text(0, 11, "WRITES ONLY 0XDE000-0XDFEFF", NOTE);
        text(0, 12, "(THE STORE'S LAST 2 SECTORS)", NOTE);
        text(0, 14, "FOR THE TRS CHECK, START A", TXT);
        text(0, 15, "SENDER ON THE TRS JACK THAT", TXT);
        text(0, 16, "NEVER PAUSES, THEN:", TXT);
        text(0, 18, "SAVE: START (ABOUT 1 MINUTE)", HEAD);
        text(0, 20, "OR INSTALL ANOTHER FIRMWARE", TXT);
        text(0, 21, "WITH THE NORMAL INSTALLER", TXT);
    } else {
        text(0, 10, "TIMES US     MIN   MEAN    MAX", HEAD);
        times(11, "ERASE 4K", &r->erase4k);
        times(12, "PROG 16", &r->prog16);
        times(13, "PROG 64", &r->prog64);
        times(14, "PROG 256", &r->prog256);
        times(15, "PAGE ER", &r->page_erase);
        times(16, "SUSPEND", &r->suspend);
        text(0, 17, "PAGE ERASE 81:", TXT);
        text(15, 17,
             r->page_erase_works == PE_PAGE      ? "THE PAGE"
             : r->page_erase_works == PE_WIDER   ? "WIDER"
             : r->page_erase_works == PE_IGNORED ? "IGNORED"
                                                 : "ODD",
             TXT);
        text(0, 18, "SUS1", TXT);
        dec(5, 18, 1, r->sus_seen, TXT);
        text(6, 18, "/5 READ", TXT);
        ok_or_count(14, 18, r->sus_read_bad);
        text(18, 18, "END", TXT);
        ok_or_count(22, 18, r->sus_unfinished);
        text(0, 19, "VERIFY", TXT);
        ok_or_count(7, 19, r->verify_bad);
        text(14, 19, "PARTIAL", TXT);
        ok_or_count(22, 19, r->partial_bad);
        text(0, 20, "REZERO", TXT);
        ok_or_count(7, 20, r->rezero_bad);
        text(14, 20, "BITWALK", TXT);
        ok_or_count(22, 20, r->bitwalk_bad);
        text(0, 21, "TWICE", TXT);
        ok_or_count(7, 21, r->twice_bad);
        text(14, 21, "GUARD", TXT);
        dec(22, 21, 1, r->refused, r->refused ? BAD : TXT);
        text(0, 22, "TRS         BYTES   LOST", HEAD);
        text(0, 23, "OPEN 1 S", TXT);
        text(0, 24, "100X 1 MS", TXT);
        text(0, 25, "10X 35 MS", TXT);
        for (uint32_t i = 0; i < 3u; i++) {
            dec(10, 23 + i, 7, r->trs[2u * i], TXT);
            dec(17, 23 + i, 7, r->trs[2u * i + 1u], r->trs[2u * i + 1u] ? BAD : TXT);
        }
    }
    if (r->state == SPIKE_RUN) {
        text(0, 27, "RUNNING, STEP", NOTE);
        dec(14, 27, 1, r->step, NOTE);
    } else if (r->state == SPIKE_DONE) {
        text(0, 27, "DONE. DF000", GOOD);
        text(12, 27, r->final_bad ? "NOT CLEAN" : "ERASED, DE000", r->final_bad ? BAD : GOOD);
        text(0, 28, "KEEPS THE PATTERN. PHOTOGRAPH", GOOD);
        text(0, 29, "THIS, THEN INSTALL AS USUAL", GOOD);
    } else if (r->state == SPIKE_REFUSED) {
        text(0, 27, "REFUSED: DE000/DF000 ARE NOT", BAD);
        text(0, 28, "THE SPIKE'S, OR ITS PATTERN", BAD);
        text(0, 29, "CHANGED. NOTHING WRITTEN.", BAD);
    } else if (r->state == SPIKE_FAILED) {
        text(0, 27, "FAILED AT STEP", BAD);
        dec(15, 27, 1, r->step, BAD);
        text(18, 27, "RC", BAD);
        dec(21, 27, 1, (uint32_t)(r->rc < 0 ? -r->rc : r->rc), BAD);
        text(0, 28, "PHOTOGRAPH, INSTALL AS USUAL", BAD);
    }
    flash_spike_show();
}

/* ---- the measurements ---- */

static void stat_add(spike_stat* s, uint32_t us) {
    if (s->n == 0u || us < s->min) {
        s->min = us;
    }
    if (us > s->max) {
        s->max = us;
    }
    s->n++;
    s->sum += us;
}

static void step(uint32_t n) {
    flash_spike_res.step = n;
    fm1_wdt_feed();
    draw();
}

static int read_sector(uint32_t off, uint8_t* dst) {
    for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i += FLASH_SPIKE_PAGE) {
        uint32_t f = irq_save();
        int rc = FL_FAR(fl_read_ram)(off + i, dst + i, FLASH_SPIKE_PAGE);
        irq_restore(f);
        if (rc) {
            return rc;
        }
    }
    return 0;
}

/* A program; 0xDF000's image follows it. */
static int prog(uint32_t off, const uint8_t* src, uint32_t n, spike_stat* s) {
    uint32_t took;
    int rc = flash_spike_prog(off, src, n, &took);
    if (rc) {
        return rc;
    }
    if (s) {
        stat_add(s, took);
    }
    if (off >= M && off < M + FLASH_SPIKE_SECTOR) {
        for (uint32_t i = 0; i < n; i++) {
            expect[off - M + i] &= src[i];
        }
    }
    return 0;
}

static void fill(uint8_t v) {
    for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
        data[i] = v;
    }
}

/* 0xDF000 erased and marked as the spike's at once. */
static int erase_m(spike_stat* s) {
    static const char mark[] = FLASH_SPIKE_MARK;
    uint32_t took;
    int rc = flash_spike_erase(M, &took);
    if (rc) {
        return rc;
    }
    if (s) {
        stat_add(s, took);
    }
    for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i++) {
        expect[i] = 0xFFu;
    }
    for (uint32_t i = 0; i < FLASH_SPIKE_MARK_LEN; i++) {
        data[i] = (uint8_t)mark[i];
    }
    return prog(M, data, FLASH_SPIKE_MARK_LEN, NULL);
}

/* Pages 1..14 of 0xDF000 programmed with zeros, each timed into s (or not). */
static int fill_m(spike_stat* s) {
    fill(0);
    for (uint32_t p = 1; p < FLASH_SPIKE_SECTOR_USE / FLASH_SPIKE_PAGE; p++) {
        int rc = prog(M + p * FLASH_SPIKE_PAGE, data, FLASH_SPIKE_PAGE, s);
        if (rc) {
            return rc;
        }
    }
    return 0;
}

/* Bytes of rd (read before) that differ from 0xDF000's image in [lo, hi). */
static uint32_t wrong(uint32_t lo, uint32_t hi) {
    uint32_t n = 0;
    for (uint32_t i = lo; i < hi; i++) {
        n += rd[i] != expect[i];
    }
    return n;
}

static int reread(void) {
    return read_sector(M, rd);
}

static uint32_t le32(const uint8_t* p) {
    return p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static void measure_part(void) {
    spike_results* r = &flash_spike_res;
    uint32_t f = irq_save();
    r->jedec = FL_FAR(fl_jedec_ram)();
    r->regs = FL_FAR(flash_spike_regs_ram)();
    FL_FAR(flash_spike_sfdp_ram)(sfdp, sizeof sfdp);
    /* 100,000 rounds of 8 adds, from SRAM with interrupts masked. */
    uint32_t ticks = FL_FAR(flash_spike_adds_ram)(100000u);
    irq_restore(f);
    r->adds_per_us = ticks ? 800000u * 24u / ticks : 0u;
    r->sfdp[0] = le32(sfdp);
    /* The first parameter header (bytes 8..15): its table's pointer in bytes 12..14. */
    uint32_t ptp = le32(&sfdp[12]) & 0xFFFFFFu;
    if (ptp + 36u <= sizeof sfdp) {
        r->sfdp[1] = le32(&sfdp[ptp]);
        r->sfdp[2] = le32(&sfdp[ptp + 28u]);
        r->sfdp[3] = le32(&sfdp[ptp + 32u]);
    }
}

/* Every store sector read once: how many are not blank. */
static int survey(void) {
    flash_spike_res.store_used = 0;
    for (uint32_t off = FLASH_SPIKE_STORE_LO; off < FLASH_SPIKE_STORE_HI;
         off += FLASH_SPIKE_SECTOR) {
        int rc = read_sector(off, rd);
        uint32_t used = 0;
        if (rc) {
            return rc;
        }
        for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i++) {
            used |= rd[i] != 0xFFu;
        }
        flash_spike_res.store_used += used;
        fm1_wdt_feed();
    }
    return 0;
}

/* 0xDE000 erased and given the mark and the pattern. */
static int write_pattern(void) {
    static const char mark[] = FLASH_SPIKE_MARK;
    uint32_t took;
    int rc = flash_spike_erase(P, &took);
    for (uint32_t p = 0; rc == 0 && p < FLASH_SPIKE_SECTOR_USE / FLASH_SPIKE_PAGE; p++) {
        for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
            uint32_t o = p * FLASH_SPIKE_PAGE + i;
            data[i] = o < FLASH_SPIKE_MARK_LEN ? (uint8_t)mark[o] : flash_spike_pattern_byte(o);
        }
        rc = prog(P + p * FLASH_SPIKE_PAGE, data, FLASH_SPIKE_PAGE, NULL);
    }
    if (rc == 0) {
        rc = read_sector(P, rd);
    }
    if (rc == 0 && flash_spike_pattern_state(rd) != FLASH_SPIKE_PATTERN_INTACT) {
        rc = -20; /* the pattern did not read back */
    }
    return rc;
}

static int timed_erases_and_pages(void) {
    for (uint32_t run = 0; run < RUNS; run++) {
        int rc = erase_m(&flash_spike_res.erase4k);
        if (rc == 0) {
            rc = fill_m(&flash_spike_res.prog256);
        }
        if (rc == 0) {
            rc = reread();
        }
        if (rc) {
            return rc;
        }
        flash_spike_res.verify_bad += wrong(0, FLASH_SPIKE_SECTOR);
        fm1_wdt_feed();
    }
    return 0;
}

/* 16-byte programs at 0x100.., 64-byte ones at 0x300..0x7FF. */
static int timed_small_programs(void) {
    int rc = erase_m(NULL);
    fill(0);
    for (uint32_t k = 0; rc == 0 && k < RUNS; k++) {
        rc = prog(M + 0x100u + 16u * k, data, 16, &flash_spike_res.prog16);
    }
    for (uint32_t k = 0; rc == 0 && k < RUNS; k++) {
        rc = prog(M + 0x300u + 64u * k, data, 64, &flash_spike_res.prog64);
    }
    if (rc == 0) {
        rc = reread();
    }
    if (rc == 0) {
        flash_spike_res.verify_bad += wrong(0, 0x800);
    }
    return rc;
}

/* Page 8: 64 programs of 4 bytes. Page 9: zeros 3 times. Page 10: 8 programs of one 16-byte
 * run, a bit cleared each time. Pages 11 and 12: programmed twice, the AND expected. */
static int reprograms(void) {
    spike_results* r = &flash_spike_res;
    int rc = 0;
    for (uint32_t k = 0; rc == 0 && k < 64u; k++) {
        for (uint32_t j = 0; j < 4u; j++) {
            data[j] = flash_spike_pattern_byte(0x800u + 4u * k + j);
        }
        rc = prog(M + 0x800u + 4u * k, data, 4, NULL);
    }
    if (rc == 0 && (rc = reread()) == 0) {
        r->partial_bad = wrong(0x800, 0x900);
    }
    fill(0);
    for (uint32_t k = 0; rc == 0 && k < 3u; k++) {
        rc = prog(M + 0x900u, data, FLASH_SPIKE_PAGE, NULL);
    }
    if (rc == 0 && (rc = reread()) == 0) {
        r->rezero_bad = wrong(0x900, 0xA00);
    }
    for (uint32_t b = 1; rc == 0 && b <= 8u; b++) {
        fill((uint8_t)(0xFFu << b));
        rc = prog(M + 0xA00u, data, 16, NULL);
        if (rc == 0 && (rc = reread()) == 0) {
            r->bitwalk_bad += wrong(0xA00, 0xB00);
        }
    }
    for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
        data[i] = flash_spike_pattern_byte(0xB00u + i);
    }
    if (rc == 0) {
        rc = prog(M + 0xB00u, data, FLASH_SPIKE_PAGE, NULL);
    }
    for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
        data[i] = flash_spike_pattern_byte(3u * i + 7u);
    }
    if (rc == 0) {
        rc = prog(M + 0xB00u, data, FLASH_SPIKE_PAGE, NULL);
    }
    for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
        data[i] = flash_spike_pattern_byte(0xC00u + i);
    }
    if (rc == 0) {
        rc = prog(M + 0xC00u, data, FLASH_SPIKE_PAGE, NULL);
    }
    data[0] = 0;
    if (rc == 0) {
        rc = prog(M + 0xC05u, data, 1, NULL); /* one byte more in a programmed page */
    }
    if (rc == 0 && (rc = reread()) == 0) {
        r->twice_bad = wrong(0xB00, 0xD00);
        /* Nothing else in the sector moved meanwhile. */
        r->verify_bad += wrong(0, 0x800) + wrong(0xD00, FLASH_SPIKE_SECTOR);
    }
    return rc;
}

/* Page 13 erased with 0x81 between programmed pages 12 and 14. */
static int page_erase(void) {
    spike_results* r = &flash_spike_res;
    uint32_t took;
    int rc;
    for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
        data[i] = flash_spike_pattern_byte(0xD00u + i);
    }
    rc = prog(M + 0xD00u, data, FLASH_SPIKE_PAGE, NULL);
    if (rc == 0) {
        rc = prog(M + 0xE00u, data, FLASH_SPIKE_PAGE, NULL);
    }
    for (uint32_t run = 0; rc == 0 && run < RUNS; run++) {
        uint32_t page_blank = 1, page_same, others;
        if (run > 0u) {
            fill(0);
            rc = prog(M + 0xD00u, data, FLASH_SPIKE_PAGE, NULL);
            if (rc) {
                break;
            }
        }
        rc = flash_spike_page_erase(M + 0xD00u, &took);
        if (rc < 0) {
            break;
        }
        if ((rc = reread()) != 0) {
            break;
        }
        for (uint32_t i = 0xD00u; i < 0xE00u; i++) {
            page_blank &= rd[i] == 0xFFu;
        }
        page_same = wrong(0xD00, 0xE00) == 0u;
        others = wrong(0, 0xD00) + wrong(0xE00, FLASH_SPIKE_SECTOR);
        if (run == 0u) {
            r->page_erase_works = page_blank && !others ? PE_PAGE
                                  : page_same && !others ? PE_IGNORED
                                  : page_blank           ? PE_WIDER
                                                         : PE_ODD;
            if (r->page_erase_works != PE_PAGE) {
                if (r->page_erase_works != PE_IGNORED) {
                    stat_add(&r->page_erase, took);
                }
                break; /* the times are only worth having for an erase of the page alone */
            }
        } else if (!page_blank || others) {
            r->page_erase_works = PE_ODD;
            break;
        }
        stat_add(&r->page_erase, took);
        for (uint32_t i = 0xD00u; i < 0xE00u; i++) {
            expect[i] = 0xFFu;
        }
        fm1_wdt_feed();
    }
    return rc;
}

static int suspends(void) {
    static const uint32_t after_us[5] = {500, 1000, 2000, 4000, 6000};
    spike_results* r = &flash_spike_res;
    flash_spike_suspend_result s;
    for (uint32_t k = 0; k < 5u; k++) {
        uint32_t from = (k + 1u) * FLASH_SPIKE_PAGE;
        int rc = erase_m(NULL);
        if (rc == 0) {
            rc = fill_m(NULL);
        }
        if (rc == 0) {
            rc = flash_spike_suspend(M, after_us[k], P + from, data, FLASH_SPIKE_PAGE, &s);
        }
        if (rc) {
            return rc;
        }
        stat_add(&r->suspend, s.latency_us);
        r->sus_seen += (s.status >> 10) & 1u;
        for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
            r->sus_read_bad += data[i] != flash_spike_pattern_byte(from + i);
        }
        if ((rc = reread()) != 0) {
            return rc;
        }
        for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i++) {
            r->sus_unfinished += rd[i] != 0xFFu;
            expect[i] = 0xFFu;
        }
        fm1_wdt_feed();
    }
    return 0;
}

/* The TRS input's counters, the input ring emptied (nothing plays it). */
static void trs(uint32_t* bytes, uint32_t* lost) {
    midi_spike_trs(bytes, lost);
}

/* ms with interrupts on, emptying the input ring. */
static void open_ms(uint32_t ms) {
    uint32_t t0 = fm1_ticks(), b, l;
    while (fm1_ticks() - t0 < ms * 1000u * FM1_TICKS_PER_US) {
        trs(&b, &l);
        __asm__ volatile("idle");
    }
    fm1_wdt_feed();
}

static void masked(uint32_t runs, uint32_t ms, uint32_t gap_ms, uint32_t* out) {
    uint32_t b0, l0, b1, l1;
    open_ms(gap_ms);
    trs(&b0, &l0);
    for (uint32_t i = 0; i < runs; i++) {
        uint32_t f = irq_save();
        fm1_delay_us(ms * 1000u);
        irq_restore(f);
        open_ms(gap_ms);
    }
    trs(&b1, &l1);
    out[0] = b1 - b0;
    out[1] = l1 - l0;
}

static void trs_check(void) {
    uint32_t* t = flash_spike_res.trs;
    uint32_t b0, l0, b1, l1;
    open_ms(10);
    trs(&b0, &l0);
    open_ms(1000);
    trs(&b1, &l1);
    t[0] = b1 - b0;
    t[1] = l1 - l0;
    masked(100, 1, 4, &t[2]);
    masked(10, 35, 15, &t[4]);
}

static int finish(void) {
    spike_results* r = &flash_spike_res;
    uint32_t took;
    int rc = flash_spike_erase(M, &took);
    if (rc == 0) {
        rc = reread();
    }
    if (rc) {
        return rc;
    }
    r->final_bad = 0;
    for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i++) {
        r->final_bad += rd[i] != 0xFFu;
    }
    if ((rc = read_sector(P, rd)) != 0) {
        return rc;
    }
    r->final_bad += flash_spike_pattern_state(rd) != FLASH_SPIKE_PATTERN_INTACT;
    return 0;
}

static int measure(void) {
    spike_results* r = &flash_spike_res;
    int rc;
    step(1);
    if ((rc = timed_erases_and_pages()) != 0) {
        return rc;
    }
    step(2);
    if ((rc = timed_small_programs()) != 0 || (rc = reprograms()) != 0) {
        return rc;
    }
    step(3);
    if ((rc = page_erase()) != 0) {
        return rc;
    }
    step(4);
    if ((rc = suspends()) != 0) {
        return rc;
    }
    step(5);
    trs_check();
    step(6);
    rc = finish();
    r->refused = flash_spike_refused;
    return rc;
}

_Noreturn static void idle_forever(void (*service)(void)) {
    flash_spike_res.refused = flash_spike_refused;
    draw();
    for (;;) {
        service();
        __asm__ volatile("idle");
    }
}

_Noreturn void flash_spike_run(void (*service)(void)) {
    spike_results* r = &flash_spike_res;
    int rc;
    for (uint32_t i = 0; i < 16u; i++) {
        palette[i] = LCD_SWAP(grid_palette[i]);
    }
    grid_init(&flash_spike_screen, ' ', TXT);
    measure_part();
    /* Read only, before anything is written: both sectors and the whole store. */
    rc = read_sector(P, rd);
    if (rc == 0) {
        r->class_pattern = (uint32_t)flash_spike_classify(rd);
        r->pattern_at_start = (uint32_t)flash_spike_pattern_state(rd);
        rc = read_sector(M, rd);
    }
    if (rc == 0) {
        r->class_measure = (uint32_t)flash_spike_classify(rd);
        rc = survey();
    }
    r->rc = rc;
    /* A damaged pattern is refused too: the spike's mark is there, but something else wrote
     * the sector since (the stock round trip's finding, kept for the user to record). */
    r->state = rc ? SPIKE_FAILED
               : r->class_pattern == FLASH_SPIKE_FOREIGN ||
                       r->class_measure == FLASH_SPIKE_FOREIGN ||
                       r->pattern_at_start == FLASH_SPIKE_PATTERN_DAMAGED
                   ? SPIKE_REFUSED
                   : SPIKE_WAIT;
    draw();
    lcd_on();
    if (r->state != SPIKE_WAIT) {
        idle_forever(service); /* nothing written */
    }
    while (!((input_buttons() >> PANEL_BTN_SAVE) & 1u)) {
        service();
        __asm__ volatile("idle");
    }
    r->state = SPIKE_RUN;
    grid_fill(&flash_spike_screen, 0, 9, GRID_COLS, GRID_ROWS - 9u, ' ', TXT);
    rc = 0;
    if (r->pattern_at_start != FLASH_SPIKE_PATTERN_INTACT) {
        step(0);
        rc = write_pattern();
    }
    if (rc == 0) {
        rc = measure();
    }
    r->rc = rc;
    r->state = rc ? SPIKE_FAILED : SPIKE_DONE;
    idle_forever(service);
}
