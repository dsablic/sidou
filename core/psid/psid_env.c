/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-License-Identifier: GPL-3.0-or-later AND BSD-2-Clause */
/* The PSID environment (psid_env.h; docs/reference/psid.md, "The environment"). The design
 * follows FM-1 SID's psid.c (BSD-2-Clause, Joakim Eriksson); its notice is in psid_env.h and
 * applies to this file as well. */
#include "psid_env.h"

#include <string.h>

#define MEM_SIZE 0x10000u
#define PORT_DDR_POWER_ON 0x2Fu
#define PORT_POWER_ON 0x37u
#define STACK 0x100u
#define KERNAL_IRQ_VECTOR 0x0314u /* the KERNAL's IRQ entry jumps through it */
#define HW_IRQ_VECTOR 0xFFFEu
#define PORT_HIRAM 0x02u   /* $01 bit 1: the KERNAL ROM at $E000 */
#define IRQ_PUSHED_P 0x20u /* an IRQ's pushed P: B clear, I clear (it was taken) */
#define SID_FIRST 0xD400u /* chip A: $D400-$D7FF, every 32 bytes a mirror */
#define SID_END 0xD800u
#define SID_WINDOW 0x20u /* a SID's registers */
#define SID_REG_MASK 0x1Fu
#define VIC_MASK 0x3Fu /* $D000-$D3FF, every 64 bytes a mirror */
#define VIC_CTRL1 0x11u
#define VIC_RASTER 0x12u
#define VIC_IRQ_FLAGS 0x19u
#define CIA1_FIRST 0xDC00u /* $DC00-$DCFF and $DD00-$DDFF, every 16 bytes a mirror */
#define CIA2_FIRST 0xDD00u
#define CIA_SIZE 0x100u
#define CIA_MIRROR_MASK 0x0Fu
#define CIA_TA_LO 0x04u
#define CIA_TA_HI 0x05u
#define CIA_ICR 0x0Du
#define IRQ_FLAGS_SET 0x81u /* an interrupt flag and its summary bit */

/* Where a call ends: the sentinel's $0000/$0001 and the KERNAL's interrupt exits, as
 * cpu6502_run's `stops` (a call that starts there has nothing to run). */
static int is_stop(uint32_t addr)
{
    return addr <= 1u || addr == CPU6502_KERNAL_IRQ_EXIT ||
           addr == CPU6502_KERNAL_IRQ_EXIT_SHORT || addr == CPU6502_KERNAL_NMI_EXIT;
}

/* --- The I/O page ------------------------------------------------------------------------
 * While $01 maps it in (the CPU checks), $D000-$DFFF goes here (docs/reference/psid.md, "The
 * I/O page"): the SIDs, VIC, CIA 1 and 2; colour RAM and the expansion areas keep nothing. */

/* Which SID a $D000-$DFFF address belongs to: 0 chip A, 1 chip B (a second SID's window, which
 * takes priority over A's mirrors), 2 a third SID's, -1 none. */
static int sid_window(const psid_env* e, uint32_t addr)
{
    const psid_header* h = e->h;
    if (h->sid2 && addr - h->sid2 < SID_WINDOW)
        return 1;
    if (h->sid3 && addr - h->sid3 < SID_WINDOW)
        return 2;
    return addr >= SID_FIRST && addr < SID_END ? 0 : -1;
}

/* CIA 1 timer A's counter: counting down from the latch, a period of latch + 1, from the
 * last underflow: a CIA song's play interrupt (call_due), else cycle 0. FM-1 SID returns the
 * latch itself. */
static uint32_t cia1_counter(const psid_env* e, uint32_t cycle)
{
    uint32_t len = (uint32_t)e->latch + 1u;
    uint32_t ref = psid_speed_bit(e->h, e->song) ? e->call_due : 0u;
    return e->latch - (cycle - ref) % len;
}

static uint8_t vic_read(psid_env* e, uint32_t reg, uint32_t cycle)
{
    uint32_t line;
    switch (reg) {
    case VIC_CTRL1:
    case VIC_RASTER:
        /* The raster line from the absolute cycle (FM-1 SID counts from each call's start). */
        line = cycle % e->frame / e->line;
        return reg == VIC_RASTER
                   ? (uint8_t)line
                   : (uint8_t)((e->vic[VIC_CTRL1] & 0x7Fu) | (line >> 1 & 0x80u));
    case VIC_IRQ_FLAGS:
        return e->vic_irq;
    default:
        return e->vic[reg];
    }
}

static uint8_t io_read(void* user, uint32_t addr, uint32_t cycle)
{
    psid_env* e = (psid_env*)user;
    uint32_t reg = addr & CIA_MIRROR_MASK;
    int w = sid_window(e, addr);
    uint8_t v;
    if (w >= 0) {
        e->sid_accesses++;
        return w < 2 && e->shadow[w] ? (uint8_t)sid_shadow_read(e->shadow[w], cycle,
                                                                 addr & SID_REG_MASK)
                                     : 0;
    }
    if (addr < SID_FIRST)
        return vic_read(e, addr & VIC_MASK, cycle);
    if (addr >= CIA1_FIRST && addr < CIA1_FIRST + CIA_SIZE) {
        if (reg == CIA_TA_LO)
            return (uint8_t)cia1_counter(e, cycle);
        if (reg == CIA_TA_HI)
            return (uint8_t)(cia1_counter(e, cycle) >> 8);
        if (reg == CIA_ICR) {
            v = e->cia1_icr; /* a read acknowledges */
            e->cia1_icr = 0;
            return v;
        }
        return e->cia[0][reg];
    }
    if (addr >= CIA2_FIRST && addr < CIA2_FIRST + CIA_SIZE)
        return reg == CIA_ICR ? 0 : e->cia[1][reg];
    return 0; /* colour RAM, the expansion areas */
}

static void io_write(void* user, uint32_t addr, uint32_t value, uint32_t cycle)
{
    psid_env* e = (psid_env*)user;
    uint32_t reg = addr & CIA_MIRROR_MASK;
    int w = sid_window(e, addr);
    if (w >= 0) {
        e->sid_accesses++;
        if (w == 2) {
            e->sid3_dropped++;
            return;
        }
        if (e->shadow[w])
            sid_shadow_write(e->shadow[w], cycle, addr & SID_REG_MASK, value);
        if (e->sink)
            e->sink(e->sink_ctx, cycle, (uint32_t)w, addr & SID_REG_MASK, value);
    } else if (addr < SID_FIRST) {
        reg = addr & VIC_MASK;
        e->vic[reg] = (uint8_t)value;
        if (reg == VIC_IRQ_FLAGS)
            e->vic_irq = 0; /* any write acknowledges */
    } else if (addr >= CIA1_FIRST && addr < CIA1_FIRST + CIA_SIZE) {
        e->cia[0][reg] = (uint8_t)value;
        if (reg == CIA_TA_LO)
            e->latch = (uint16_t)((e->latch & 0xFF00u) | value);
        else if (reg == CIA_TA_HI)
            e->latch = (uint16_t)((e->latch & 0x00FFu) | value << 8);
    } else if (addr >= CIA2_FIRST && addr < CIA2_FIRST + CIA_SIZE) {
        e->cia[1][reg] = (uint8_t)value;
    }
}

/* --- Loading ------------------------------------------------------------------------------ */

void psid_env_load(psid_env* e, uint8_t* mem, const psid_header* h, const uint8_t* file)
{
    int ntsc = h->clock == 2u;
    memset(e, 0, sizeof *e);
    e->h = h;
    e->cpu.mem = mem;
    e->cpu.rd = io_read;
    e->cpu.wr = io_write;
    e->cpu.user = e;
    e->cpu.io = 1;
    e->cpu.p = 0x24u;
    e->clock = ntsc ? PSID_CLOCK_NTSC : PSID_CLOCK_PAL;
    e->frame = (uint16_t)(ntsc ? PSID_FRAME_NTSC : PSID_FRAME_PAL);
    e->line = (uint8_t)(ntsc ? PSID_LINE_NTSC : PSID_LINE_PAL);
    e->latch = (uint16_t)(ntsc ? PSID_LATCH_NTSC : PSID_LATCH_PAL);
    memset(mem, 0, MEM_SIZE);
    mem[0] = PORT_DDR_POWER_ON;
    mem[1] = PORT_POWER_ON;
    if (file)
        psid_env_load_part(e, 0, file, h->data_off + h->data_len);
}

void psid_env_load_part(psid_env* e, uint32_t offset, const uint8_t* bytes, uint32_t n)
{
    const psid_header* h = e->h;
    uint32_t lo, hi;
    if (!h || !e->cpu.mem)
        return;
    lo = h->data_off;
    hi = h->data_off + h->data_len; /* psid_parse: within the file, and load + len <= $10000 */
    if (n == 0u || offset >= hi)
        return;
    if (n > hi - offset)
        n = hi - offset;
    if (offset < lo) {
        if (lo - offset >= n)
            return;
        bytes += lo - offset;
        n -= lo - offset;
        offset = lo;
    }
    memcpy(e->cpu.mem + h->load + (offset - lo), bytes, n);
}

void psid_env_set_shadows(psid_env* e, sid_shadow* a, sid_shadow* b)
{
    e->shadow[0] = a;
    e->shadow[1] = b;
}

void psid_env_set_sink(psid_env* e, psid_sink sink, void* ctx)
{
    e->sink = sink;
    e->sink_ctx = ctx;
}

/* --- Calls -------------------------------------------------------------------------------- */

uint8_t psid_env_bank(uint32_t addr)
{
    if (addr < 0xA000u)
        return 0x37u;
    if (addr < 0xD000u)
        return 0x36u;
    if (addr < 0xE000u)
        return 0x34u;
    return 0x35u;
}

uint32_t psid_env_period(const psid_env* e)
{
    if (psid_speed_bit(e->h, e->song))
        return (uint32_t)e->latch + 1u;
    return e->frame;
}

/* Starts a call at the current cycle: the registers as a call starts (A, X, Y 0, I set, D
 * clear), the sentinel $0000 pushed below S = $FF so an RTS lands on $0001. A call to an
 * address sets the bank register from it. An interrupt call (play address 0) leaves $01 as
 * the tune set it and goes through the vector it maps in, with the stack as an IRQ leaves it
 * (P pushed); through the KERNAL's $0314, also as the KERNAL's entry ($FF48) leaves it: A, X
 * and Y pushed and X = S, so a handler may end with the KERNAL exit's own PLA, TAY, PLA,
 * TAX, PLA, RTI. */
static void begin(psid_env* e, uint32_t addr, uint32_t a, int irq)
{
    cpu6502* c = &e->cpu;
    uint8_t* mem = c->mem;
    uint32_t s = 0xFFu;
    cpu6502_reset_regs(c);
    mem[STACK | s--] = 0;
    mem[STACK | s--] = 0;
    if (irq) {
        mem[STACK | s--] = IRQ_PUSHED_P;
        if (mem[1] & PORT_HIRAM) {
            addr = mem[KERNAL_IRQ_VECTOR] | (uint32_t)mem[KERNAL_IRQ_VECTOR + 1u] << 8;
            mem[STACK | s--] = 0; /* A, X and Y */
            mem[STACK | s--] = 0;
            mem[STACK | s--] = 0;
            c->x = (uint8_t)s;
        } else {
            addr = mem[HW_IRQ_VECTOR] | (uint32_t)mem[HW_IRQ_VECTOR + 1u] << 8;
        }
    } else {
        mem[1] = psid_env_bank(addr);
    }
    c->s = (uint8_t)s;
    c->a = (uint8_t)a;
    c->pc = (uint16_t)addr;
    if (is_stop(addr))
        c->state = CPU6502_RET;
    e->call_start = c->cycles;
}

int psid_env_start(psid_env* e, uint32_t song)
{
    const psid_header* h = e->h;
    if (!h || !e->cpu.mem)
        return -1;
    if (song == 0u)
        song = h->start;
    if (song < 1u || song > h->songs)
        return -1;
    e->song = (uint16_t)song;
    e->latch = (uint16_t)(e->clock == PSID_CLOCK_NTSC ? PSID_LATCH_NTSC : PSID_LATCH_PAL);
    e->calls = 0;
    e->overruns = 0;
    e->late = 0;
    e->sid3_dropped = 0;
    memset(e->vic, 0, sizeof e->vic);
    memset(e->cia, 0, sizeof e->cia);
    e->vic_irq = 0;
    e->cia1_icr = 0;
    e->cpu.cycles = 0;
    e->call_due = 0;
    e->next_due = 0;
    e->next_grid = 0;
    e->stop = 0;
    e->phase = PSID_PHASE_INIT;
    begin(e, h->init, song - 1u, 0);
    return 0;
}

static int stop(psid_env* e, int code)
{
    e->phase = PSID_PHASE_STOP;
    e->stop = (uint8_t)code;
    return code;
}

/* A call returned at the current cycle: schedules the next play call. The first after init is
 * at the first period boundary after it; each next one a period after the last one's
 * interrupt. A call that ran past that is an overrun: the interrupts it missed are one pending
 * interrupt, taken at its return, as a C64's are (the IRQ line stays low until acknowledged),
 * and the ones after it keep their grid. */
static int returned(psid_env* e)
{
    uint32_t now = e->cpu.cycles;
    uint32_t p = psid_env_period(e);
    uint32_t grid;
    int code;
    if (e->phase == PSID_PHASE_INIT) {
        grid = (now / p + 1u) * p;
        e->next_due = grid;
        code = PSID_RUN_INIT_DONE;
    } else {
        grid = e->call_due + p;
        e->next_due = grid;
        if ((int32_t)(now - grid) > 0) {
            e->overruns++;
            grid += (now - grid) / p * p;
            e->next_due = now;
        }
        code = PSID_RUN_CALL_DONE;
    }
    e->next_grid = grid;
    e->phase = PSID_PHASE_WAIT;
    return code;
}

int psid_env_run(psid_env* e, uint32_t cycles)
{
    cpu6502* c = &e->cpu;
    uint32_t end = c->cycles + cycles;
    for (;;) {
        uint32_t limit;
        int r;
        switch (e->phase) {
        case PSID_PHASE_WAIT:
            if ((int32_t)(e->next_due - end) > 0) {
                if ((int32_t)(end - c->cycles) > 0)
                    c->cycles = end;
                return PSID_RUN_IDLE;
            }
            if ((int32_t)(e->next_due - c->cycles) > 0)
                c->cycles = e->next_due;
            e->call_due = e->next_grid;
            e->calls++;
            e->phase = PSID_PHASE_PLAY;
            begin(e, e->h->play, 0, e->h->play == 0u);
            /* The interrupt that started the call: CIA 1 timer A's or the frame's (VIC's). */
            if (psid_speed_bit(e->h, e->song))
                e->cia1_icr = IRQ_FLAGS_SET;
            else
                e->vic_irq = IRQ_FLAGS_SET;
            continue;
        case PSID_PHASE_INIT:
        case PSID_PHASE_PLAY:
            limit = e->call_start +
                    (e->phase == PSID_PHASE_INIT ? PSID_INIT_SECONDS * e->clock : e->clock);
            if (c->state == CPU6502_RET)
                r = CPU6502_RET; /* a call to a stop address */
            else
                r = cpu6502_run(c, (int32_t)(limit - end) < 0 ? limit : end, 1);
            if (r == CPU6502_JAM)
                return stop(e, PSID_RUN_JAM);
            if (r == CPU6502_RET)
                return returned(e);
            if ((int32_t)(c->cycles - limit) >= 0)
                return stop(e, e->phase == PSID_PHASE_INIT ? PSID_RUN_INIT_TOO_LONG
                                                           : PSID_RUN_PLAY_HUNG);
            return PSID_RUN_MORE;
        case PSID_PHASE_STOP:
            return e->stop;
        default:
            return PSID_RUN_NOT_STARTED;
        }
    }
}
