// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stddef.h>
#include <stdint.h>

#include "fm1_sys.h"
#include "fm1_time.h"

#include "flash_spike_guard.h"
#define FL_RANGE_OK(off, n) FLASH_SPIKE_DRIVER_OK(off, n)
#include "fm1_flash.h"

#include "flash_spike.h"
#include "font.h"
#include "grid.h"
#include "input.h"
#include "lcd.h"
#include "midi.h"
#include "panel.h"

RAMINL void flash_spike_wait_us(uint32_t us) {
    uint32_t t0 = T4_CNT;
    while ((T4_CNT - t0) / 24u < us) {
    }
}

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
        fl_inval(off & ~0x1FFu, 2u * FLASH_SPIKE_PAGE);
        rc = fl_wait_ready(100000u, took_us);
        if (rc == 0 && (fl_rdsr(0x05) & 2u)) {
            fl_cs(0);
            fl_tx(0x04);
            fl_cs(1);
            rc = 1;
        }
    }
    fl_exit(&s);
    return rc;
}

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

#define SPIKE_BOOT 0u
#define SPIKE_WAIT 1u
#define SPIKE_RUN 2u
#define SPIKE_DONE 3u
#define SPIKE_REFUSED 4u
#define SPIKE_FAILED 5u

#define PE_IGNORED 0u
#define PE_PAGE 1u
#define PE_WIDER 2u
#define PE_ODD 3u

typedef struct {
    uint32_t n, min, max, sum;
} spike_stat;

typedef struct {
    uint32_t state;
    uint32_t step;
    int32_t rc;
    uint32_t refused;
    uint32_t jedec;
    uint32_t regs;
    uint32_t adds_per_us;
    uint32_t class_pattern;
    uint32_t class_measure;
    uint32_t pattern_at_start;
    uint32_t store_used;
    spike_stat erase4k;
    spike_stat prog16;
    spike_stat prog64;
    spike_stat prog256;
    spike_stat page_erase;
    uint32_t page_erase_works;
    spike_stat suspend;
    uint32_t sus_seen;
    uint32_t sus_read_bad;
    uint32_t sus_unfinished;
    uint32_t verify_bad;
    uint32_t partial_bad;
    uint32_t rezero_bad;
    uint32_t bitwalk_bad;
    uint32_t twice_bad;
    uint32_t trs[6];
    uint32_t sfdp[4];
    uint32_t final_bad;
} spike_results;
static spike_results flash_spike_res __attribute__((used));

#define SPIKE_SFDP_BYTES 128u
typedef struct {
    uint8_t rd[FLASH_SPIKE_SECTOR];
    uint8_t expect[FLASH_SPIKE_SECTOR];
    uint8_t data[FLASH_SPIKE_PAGE];
    uint8_t sfdp[SPIKE_SFDP_BYTES];
    grid screen;
} spike_buffers;
extern uint32_t __song_region[];
_Static_assert(sizeof(spike_buffers) <= 65536u, "the spike's buffers must fit the song's region");
#define SPIKE_BUF ((spike_buffers*)(void*)__song_region)
static uint8_t* const rd = SPIKE_BUF->rd;
static uint8_t* const expect = SPIKE_BUF->expect;
static uint8_t* const data = SPIKE_BUF->data;
static uint8_t* const sfdp = SPIKE_BUF->sfdp;
#define flash_spike_screen (SPIKE_BUF->screen)
static grid* const flash_spike_screen_at __attribute__((used)) = &SPIKE_BUF->screen;
static uint16_t palette[16];

#define M FLASH_SPIKE_MEASURE
#define P FLASH_SPIKE_PATTERN
#define RUNS 20u

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
    FL_FAR(flash_spike_sfdp_ram)(sfdp, SPIKE_SFDP_BYTES);
    uint32_t ticks = FL_FAR(flash_spike_adds_ram)(100000u);
    irq_restore(f);
    r->adds_per_us = ticks ? 800000u * 24u / ticks : 0u;
    r->sfdp[0] = le32(sfdp);
    uint32_t ptp = le32(&sfdp[12]) & 0xFFFFFFu;
    if (ptp + 36u <= SPIKE_SFDP_BYTES) {
        r->sfdp[1] = le32(&sfdp[ptp]);
        r->sfdp[2] = le32(&sfdp[ptp + 28u]);
        r->sfdp[3] = le32(&sfdp[ptp + 32u]);
    }
}

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
        rc = -20;
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
        rc = prog(M + 0xC05u, data, 1, NULL);
    }
    if (rc == 0 && (rc = reread()) == 0) {
        r->twice_bad = wrong(0xB00, 0xD00);
        r->verify_bad += wrong(0, 0x800) + wrong(0xD00, FLASH_SPIKE_SECTOR);
    }
    return rc;
}

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
                break;
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

static void trs(uint32_t* bytes, uint32_t* lost) {
    midi_spike_trs(bytes, lost);
}

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
    r->state = rc ? SPIKE_FAILED
               : r->class_pattern == FLASH_SPIKE_FOREIGN ||
                       r->class_measure == FLASH_SPIKE_FOREIGN ||
                       r->pattern_at_start == FLASH_SPIKE_PATTERN_DAMAGED
                   ? SPIKE_REFUSED
                   : SPIKE_WAIT;
    draw();
    lcd_on();
    if (r->state != SPIKE_WAIT) {
        idle_forever(service);
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
