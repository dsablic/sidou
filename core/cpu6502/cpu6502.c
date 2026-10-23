// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's 6502 (cpu6502.h): one switch over the opcodes, the registers in locals, the flags
 * unpacked (N and Z kept as the last result's byte), the cycle count added from a table before
 * an instruction runs so its accesses can be stamped back from the end. */
#include "cpu6502.h"

/* Base cycles per opcode (NMOS); page crossings and taken branches add theirs. JAMs are 0. */
static const uint8_t cpu6502_cycles[256] = {
    7, 6, 0, 8, 3, 3, 5, 5, 3, 2, 2, 2, 4, 4, 6, 6, 2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    6, 6, 0, 8, 3, 3, 5, 5, 4, 2, 2, 2, 4, 4, 6, 6, 2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    6, 6, 0, 8, 3, 3, 5, 5, 3, 2, 2, 2, 3, 4, 6, 6, 2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    6, 6, 0, 8, 3, 3, 5, 5, 4, 2, 2, 2, 5, 4, 6, 6, 2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, 2, 6, 0, 6, 4, 4, 4, 4, 2, 5, 2, 5, 5, 5, 5, 5,
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, 2, 5, 0, 5, 4, 4, 4, 4, 2, 4, 2, 4, 4, 4, 4, 4,
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, 2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, 2, 5, 0, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
};

/* --- Memory accesses ---------------------------------------------------------------------
 * An operand access to $D000-$DFFF goes to a hook while `io` is set and $01 maps I/O in; the
 * zero page and the stack never are I/O. The stamps count back from the instruction's end:
 * `cyc` already holds its last cycle + 1. CPU6502_HOOK_ALL (the tests') makes the address and
 * bank tests true and sends the zero page's and the stack's writes through the hooks too. */
#ifdef CPU6502_HOOK_ALL
#define IO_ON(b) ((void)(b), 1)
#define IN_IO(ea) ((void)(ea), 1)
#define HOOK_ZP 1
#else
#define IO_ON(b) (((b) & 3u) && ((b) & 4u))
#define IN_IO(ea) ((((ea) & 0xF000u) == 0xD000u) && io)
#define HOOK_ZP 0
#endif
#define IS_IO(ea) (IN_IO(ea) && IO_ON(mem[1]))

#define FETCH() (t8 = mem[pc], pc = (pc + 1u) & 0xFFFFu, t8)
#define ABS16() (lo = FETCH(), (uint32_t)lo | (uint32_t)FETCH() << 8)

#ifndef CPU6502_SMALL
/* RDK: a read `k` cycles from the end; WR: a write on the last; WR2: a read-modify-write's
 * two writes, the old value, then the new one */
#define RDK(ea, k) (IS_IO(ea) ? c->rd(c->user, (ea), cyc - (k)) : mem[(ea)])
#define WR(ea, v)                                                                                  \
    do {                                                                                           \
        if (IS_IO(ea))                                                                             \
            c->wr(c->user, (ea), (v), cyc - 1u);                                                   \
        else                                                                                       \
            mem[(ea)] = (uint8_t)(v);                                                              \
    } while (0)
#define WR2(ea, old, v)                                                                            \
    do {                                                                                           \
        if (IS_IO(ea)) {                                                                           \
            c->wr(c->user, (ea), (old), cyc - 2u);                                                 \
            c->wr(c->user, (ea), (v), cyc - 1u);                                                   \
        } else {                                                                                   \
            mem[(ea)] = (uint8_t)(v);                                                              \
        }                                                                                          \
    } while (0)
#else
/* The I/O page's accesses out of line: one compare inline. */
__attribute__((noinline)) static uint32_t rd_slow(cpu6502* c, uint32_t ea, uint32_t cycle) {
    return IO_ON(c->mem[1]) ? c->rd(c->user, ea, cycle) : c->mem[ea];
}
__attribute__((noinline)) static void wr_slow(cpu6502* c, uint32_t ea, uint32_t old, uint32_t v,
                                              uint32_t cycle, int rmw) {
    if (!IO_ON(c->mem[1])) {
        c->mem[ea] = (uint8_t)v;
        return;
    }
    if (rmw)
        c->wr(c->user, ea, old, cycle - 1u);
    c->wr(c->user, ea, v, cycle);
}
#define RDK(ea, k) (IN_IO(ea) ? rd_slow(c, (ea), cyc - (k)) : mem[(ea)])
#define WR(ea, v)                                                                                  \
    do {                                                                                           \
        if (IN_IO(ea))                                                                             \
            wr_slow(c, (ea), 0, (v), cyc - 1u, 0);                                                 \
        else                                                                                       \
            mem[(ea)] = (uint8_t)(v);                                                              \
    } while (0)
#define WR2(ea, old, v)                                                                            \
    do {                                                                                           \
        if (IN_IO(ea))                                                                             \
            wr_slow(c, (ea), (old), (v), cyc - 1u, 1);                                             \
        else                                                                                       \
            mem[(ea)] = (uint8_t)(v);                                                              \
    } while (0)
#endif
#define RD(ea) RDK(ea, 1u)

/* The zero page: memory, or the hooks under CPU6502_HOOK_ALL. */
#define ZRDK(ea, k) (HOOK_ZP ? RDK(ea, k) : mem[(ea)])
#define ZWR(ea, v)                                                                                 \
    do {                                                                                           \
        if (HOOK_ZP)                                                                               \
            WR(ea, v);                                                                             \
        else                                                                                       \
            mem[(ea)] = (uint8_t)(v);                                                              \
    } while (0)
#define ZWR2(ea, old, v)                                                                           \
    do {                                                                                           \
        if (HOOK_ZP)                                                                               \
            WR2(ea, old, v);                                                                       \
        else                                                                                       \
            mem[(ea)] = (uint8_t)(v);                                                              \
    } while (0)

/* The stack: a push `k` cycles from the end (stamped only under CPU6502_HOOK_ALL); pulls
 * read memory. */
#define PUSH(v, k)                                                                                 \
    do {                                                                                           \
        if (HOOK_ZP)                                                                               \
            c->wr(c->user, 0x100u | s, (v) & 0xFFu, cyc - (k));                                    \
        else                                                                                       \
            mem[0x100u | s] = (uint8_t)(v);                                                        \
        s = (s - 1u) & 0xFFu;                                                                      \
    } while (0)
#define PULL() (s = (s + 1u) & 0xFFu, mem[0x100u | s])
#define NZ(v) (n = z = (v))

/* Effective addresses; the *_R forms add the page-crossing cycle. */
#define EA_ZP() (ea = FETCH())
#define EA_ZPX() (ea = (FETCH() + x) & 0xFFu)
#define EA_ZPY() (ea = (FETCH() + y) & 0xFFu)
#define EA_ABS() (ea = ABS16())
#define EA_ABX() (ea = (ABS16() + x) & 0xFFFFu)
#define EA_ABY() (ea = (ABS16() + y) & 0xFFFFu)
#define EA_ABX_R() (t = ABS16(), ea = (t + x) & 0xFFFFu, cyc += ((t ^ ea) >> 8) & 1u)
#define EA_ABY_R() (t = ABS16(), ea = (t + y) & 0xFFFFu, cyc += ((t ^ ea) >> 8) & 1u)
#define EA_IZX() (t = (FETCH() + x) & 0xFFu, ea = mem[t] | (uint32_t)mem[(t + 1u) & 0xFFu] << 8)
#define EA_IZY() (t = FETCH(), ea = ((mem[t] | (uint32_t)mem[(t + 1u) & 0xFFu] << 8) + y) & 0xFFFFu)
#define EA_IZY_R()                                                                                 \
    (t = FETCH(), t = mem[t] | (uint32_t)mem[(t + 1u) & 0xFFu] << 8, ea = (t + y) & 0xFFFFu,       \
     cyc += ((t ^ ea) >> 8) & 1u)

/* The operations on v, a read operand. */
#define OP_ORA() (a |= v, NZ(a))
#define OP_AND() (a &= v, NZ(a))
#define OP_EOR() (a ^= v, NZ(a))
#define OP_LDA() (a = v, NZ(a))
#define OP_LDX() (x = v, NZ(x))
#define OP_LDY() (y = v, NZ(y))
#define OP_LAX() (a = x = v, NZ(a))
#define OP_CMP(r) (t = (r) - v, cf = (r) >= v, NZ(t & 0xFFu))
#define OP_BIT() (z = a & v, n = v, vf = (v >> 6) & 1u)
/* NMOS decimal mode: Z from the binary sum, N and V from the high digit before its carry
 * adjustment. */
#define OP_ADC()                                                                                   \
    do {                                                                                           \
        uint32_t r_ = a + v + cf;                                                                  \
        if (!df) {                                                                                 \
            vf = ((~(a ^ v) & (a ^ r_)) >> 7) & 1u;                                                \
            cf = r_ >> 8;                                                                          \
            a = r_ & 0xFFu;                                                                        \
            n = z = a;                                                                             \
        } else {                                                                                   \
            uint32_t lo_ = (a & 0x0Fu) + (v & 0x0Fu) + cf;                                         \
            uint32_t hi_ = (a >> 4) + (v >> 4);                                                    \
            if (lo_ > 9u)                                                                          \
                lo_ += 6u;                                                                         \
            if (lo_ > 0x0Fu)                                                                       \
                hi_++;                                                                             \
            z = r_ & 0xFFu;                                                                        \
            n = (hi_ << 4) & 0xFFu;                                                                \
            vf = ((~(a ^ v) & (a ^ (hi_ << 4))) >> 7) & 1u;                                        \
            if (hi_ > 9u)                                                                          \
                hi_ += 6u;                                                                         \
            cf = hi_ > 0x0Fu;                                                                      \
            a = ((hi_ << 4) | (lo_ & 0x0Fu)) & 0xFFu;                                              \
        }                                                                                          \
    } while (0)
/* NMOS decimal mode: every flag from the binary difference. */
#define OP_SBC()                                                                                   \
    do {                                                                                           \
        uint32_t r_ = a - v - (1u - cf);                                                           \
        vf = (((a ^ v) & (a ^ r_)) >> 7) & 1u;                                                     \
        n = z = r_ & 0xFFu;                                                                        \
        if (df) {                                                                                  \
            uint32_t lo_ = (a & 0x0Fu) - (v & 0x0Fu) - (1u - cf);                                  \
            uint32_t hi_ = (a >> 4) - (v >> 4);                                                    \
            if (lo_ & 0x10u) {                                                                     \
                lo_ -= 6u;                                                                         \
                hi_--;                                                                             \
            }                                                                                      \
            if (hi_ & 0x10u)                                                                       \
                hi_ -= 6u;                                                                         \
            a = ((hi_ << 4) | (lo_ & 0x0Fu)) & 0xFFu;                                              \
        } else {                                                                                   \
            a = r_ & 0xFFu;                                                                        \
        }                                                                                          \
        cf = (r_ >> 8) == 0u;                                                                      \
    } while (0)
#define OP_ASL() (cf = v >> 7, v = (v << 1) & 0xFFu, NZ(v))
#define OP_LSR() (cf = v & 1u, v >>= 1, NZ(v))
#define OP_ROL() (t = cf, cf = v >> 7, v = ((v << 1) | t) & 0xFFu, NZ(v))
#define OP_ROR() (t = cf, cf = v & 1u, v = (v >> 1) | t << 7, NZ(v))
#define OP_INC() (v = (v + 1u) & 0xFFu, NZ(v))
#define OP_DEC() (v = (v - 1u) & 0xFFu, NZ(v))

/* A read instruction in each addressing mode. */
#define R_IMM(op)                                                                                  \
    v = FETCH();                                                                                   \
    op;                                                                                            \
    break
#define R_ZP(op)                                                                                   \
    EA_ZP();                                                                                       \
    v = ZRDK(ea, 1u);                                                                              \
    op;                                                                                            \
    break
#define R_ZPX(op)                                                                                  \
    EA_ZPX();                                                                                      \
    v = ZRDK(ea, 1u);                                                                              \
    op;                                                                                            \
    break
#define R_ZPY(op)                                                                                  \
    EA_ZPY();                                                                                      \
    v = ZRDK(ea, 1u);                                                                              \
    op;                                                                                            \
    break
#define R_ABS(op)                                                                                  \
    EA_ABS();                                                                                      \
    v = RD(ea);                                                                                    \
    op;                                                                                            \
    break
#define R_ABX(op)                                                                                  \
    EA_ABX_R();                                                                                    \
    v = RD(ea);                                                                                    \
    op;                                                                                            \
    break
#define R_ABY(op)                                                                                  \
    EA_ABY_R();                                                                                    \
    v = RD(ea);                                                                                    \
    op;                                                                                            \
    break
#define R_IZX(op)                                                                                  \
    EA_IZX();                                                                                      \
    v = RD(ea);                                                                                    \
    op;                                                                                            \
    break
#define R_IZY(op)                                                                                  \
    EA_IZY_R();                                                                                    \
    v = RD(ea);                                                                                    \
    op;                                                                                            \
    break

/* A store in each addressing mode. */
#define W_ZP(r)                                                                                    \
    EA_ZP();                                                                                       \
    ZWR(ea, (r));                                                                                  \
    break
#define W_ZPX(r)                                                                                   \
    EA_ZPX();                                                                                      \
    ZWR(ea, (r));                                                                                  \
    break
#define W_ZPY(r)                                                                                   \
    EA_ZPY();                                                                                      \
    ZWR(ea, (r));                                                                                  \
    break
#define W_ABS(r)                                                                                   \
    EA_ABS();                                                                                      \
    WR(ea, (r));                                                                                   \
    break
#define W_ABX(r)                                                                                   \
    EA_ABX();                                                                                      \
    WR(ea, (r));                                                                                   \
    break
#define W_ABY(r)                                                                                   \
    EA_ABY();                                                                                      \
    WR(ea, (r));                                                                                   \
    break
#define W_IZX(r)                                                                                   \
    EA_IZX();                                                                                      \
    WR(ea, (r));                                                                                   \
    break
#define W_IZY(r)                                                                                   \
    EA_IZY();                                                                                      \
    WR(ea, (r));                                                                                   \
    break

/* A read-modify-write: the read on the third last cycle, the two writes on the last two;
 * `op2` (the undocumented combinations' second operation) runs after them. */
#define M_ZPEA(op, op2)                                                                            \
    old = v = ZRDK(ea, 3u);                                                                        \
    op;                                                                                            \
    ZWR2(ea, old, v);                                                                              \
    op2;                                                                                           \
    break
#define M_EA(op, op2)                                                                              \
    old = v = RDK(ea, 3u);                                                                         \
    op;                                                                                            \
    WR2(ea, old, v);                                                                               \
    op2;                                                                                           \
    break
#define M_ZP(op, op2)                                                                              \
    EA_ZP();                                                                                       \
    M_ZPEA(op, op2)
#define M_ZPX(op, op2)                                                                             \
    EA_ZPX();                                                                                      \
    M_ZPEA(op, op2)
#define M_ABS(op, op2)                                                                             \
    EA_ABS();                                                                                      \
    M_EA(op, op2)
#define M_ABX(op, op2)                                                                             \
    EA_ABX();                                                                                      \
    M_EA(op, op2)
#define M_ABY(op, op2)                                                                             \
    EA_ABY();                                                                                      \
    M_EA(op, op2)
#define M_IZX(op, op2)                                                                             \
    EA_IZX();                                                                                      \
    M_EA(op, op2)
#define M_IZY(op, op2)                                                                             \
    EA_IZY();                                                                                      \
    M_EA(op, op2)

/* A branch: a cycle more when taken, another when it lands on another page. */
#define BRANCH(cond)                                                                               \
    t = FETCH();                                                                                   \
    if (cond) {                                                                                    \
        ea = (pc + t - ((t & 0x80u) << 1)) & 0xFFFFu;                                              \
        cyc += 1u + (((ea ^ pc) >> 8) & 1u);                                                       \
        pc = ea;                                                                                   \
    }                                                                                              \
    break

/* The unstable stores: the value & (the base's high byte + 1); when the index crosses a
 * page, that value is also the address's high byte. */
#define SH_STORE(base, idx, val)                                                                   \
    t = (base);                                                                                    \
    ea = (t + (idx)) & 0xFFFFu;                                                                    \
    v = (val) & ((t >> 8) + 1u) & 0xFFu;                                                           \
    if ((t ^ ea) & 0x100u)                                                                         \
        ea = (ea & 0xFFu) | v << 8;                                                                \
    WR(ea, v);                                                                                     \
    break

#define PACK_P()                                                                                   \
    ((n & 0x80u) | vf << 6 | 0x20u | df << 3 | ifl << 2 | (uint32_t)((z & 0xFFu) == 0u) << 1 | cf)
#define UNPACK_P(pv)                                                                               \
    (n = (pv), vf = ((pv) >> 6) & 1u, df = ((pv) >> 3) & 1u, ifl = ((pv) >> 2) & 1u,               \
     z = ((pv) & 2u) ^ 2u, cf = (pv) & 1u)

/* Where a call ends (with `stops`): the sentinel's return ($0000 pushed, so RTS lands on
 * $0001) and the KERNAL's interrupt exits. */
#define STOPS(addr)                                                                                \
    (stops && ((addr) <= 1u || (addr) == CPU6502_KERNAL_IRQ_EXIT ||                                \
               (addr) == CPU6502_KERNAL_IRQ_EXIT_SHORT || (addr) == CPU6502_KERNAL_NMI_EXIT))
#define JUMPED()                                                                                   \
    if (STOPS(pc)) {                                                                               \
        state = CPU6502_RET;                                                                       \
        goto out;                                                                                  \
    }                                                                                              \
    break

int cpu6502_run(cpu6502* c, uint32_t limit, int stops) {
    uint8_t* mem = c->mem;
    uint32_t pc = c->pc, a = c->a, x = c->x, y = c->y, s = c->s;
    uint32_t n, z, vf, df, ifl, cf;
    uint32_t cyc = c->cycles, io = c->io, p0 = c->p;
    uint32_t ea, t, v, old, lo;
    uint8_t t8;
    int state = CPU6502_RUN;
    (void)io;
    UNPACK_P(p0);
    while ((int32_t)(cyc - limit) < 0) {
        uint32_t op = FETCH();
        cyc += cpu6502_cycles[op];
        switch (op) {
        /* ORA */
        case 0x09: R_IMM(OP_ORA());
        case 0x05: R_ZP(OP_ORA());
        case 0x15: R_ZPX(OP_ORA());
        case 0x0D: R_ABS(OP_ORA());
        case 0x1D: R_ABX(OP_ORA());
        case 0x19: R_ABY(OP_ORA());
        case 0x01: R_IZX(OP_ORA());
        case 0x11: R_IZY(OP_ORA());
        /* AND */
        case 0x29: R_IMM(OP_AND());
        case 0x25: R_ZP(OP_AND());
        case 0x35: R_ZPX(OP_AND());
        case 0x2D: R_ABS(OP_AND());
        case 0x3D: R_ABX(OP_AND());
        case 0x39: R_ABY(OP_AND());
        case 0x21: R_IZX(OP_AND());
        case 0x31: R_IZY(OP_AND());
        /* EOR */
        case 0x49: R_IMM(OP_EOR());
        case 0x45: R_ZP(OP_EOR());
        case 0x55: R_ZPX(OP_EOR());
        case 0x4D: R_ABS(OP_EOR());
        case 0x5D: R_ABX(OP_EOR());
        case 0x59: R_ABY(OP_EOR());
        case 0x41: R_IZX(OP_EOR());
        case 0x51: R_IZY(OP_EOR());
        /* ADC */
        case 0x69: R_IMM(OP_ADC());
        case 0x65: R_ZP(OP_ADC());
        case 0x75: R_ZPX(OP_ADC());
        case 0x6D: R_ABS(OP_ADC());
        case 0x7D: R_ABX(OP_ADC());
        case 0x79: R_ABY(OP_ADC());
        case 0x61: R_IZX(OP_ADC());
        case 0x71: R_IZY(OP_ADC());
        /* SBC, and the undocumented $EB */
        case 0xE9:
        case 0xEB: R_IMM(OP_SBC());
        case 0xE5: R_ZP(OP_SBC());
        case 0xF5: R_ZPX(OP_SBC());
        case 0xED: R_ABS(OP_SBC());
        case 0xFD: R_ABX(OP_SBC());
        case 0xF9: R_ABY(OP_SBC());
        case 0xE1: R_IZX(OP_SBC());
        case 0xF1: R_IZY(OP_SBC());
        /* CMP, CPX, CPY */
        case 0xC9: R_IMM(OP_CMP(a));
        case 0xC5: R_ZP(OP_CMP(a));
        case 0xD5: R_ZPX(OP_CMP(a));
        case 0xCD: R_ABS(OP_CMP(a));
        case 0xDD: R_ABX(OP_CMP(a));
        case 0xD9: R_ABY(OP_CMP(a));
        case 0xC1: R_IZX(OP_CMP(a));
        case 0xD1: R_IZY(OP_CMP(a));
        case 0xE0: R_IMM(OP_CMP(x));
        case 0xE4: R_ZP(OP_CMP(x));
        case 0xEC: R_ABS(OP_CMP(x));
        case 0xC0: R_IMM(OP_CMP(y));
        case 0xC4: R_ZP(OP_CMP(y));
        case 0xCC: R_ABS(OP_CMP(y));
        /* BIT */
        case 0x24: R_ZP(OP_BIT());
        case 0x2C: R_ABS(OP_BIT());
        /* Loads */
        case 0xA9: R_IMM(OP_LDA());
        case 0xA5: R_ZP(OP_LDA());
        case 0xB5: R_ZPX(OP_LDA());
        case 0xAD: R_ABS(OP_LDA());
        case 0xBD: R_ABX(OP_LDA());
        case 0xB9: R_ABY(OP_LDA());
        case 0xA1: R_IZX(OP_LDA());
        case 0xB1: R_IZY(OP_LDA());
        case 0xA2: R_IMM(OP_LDX());
        case 0xA6: R_ZP(OP_LDX());
        case 0xB6: R_ZPY(OP_LDX());
        case 0xAE: R_ABS(OP_LDX());
        case 0xBE: R_ABY(OP_LDX());
        case 0xA0: R_IMM(OP_LDY());
        case 0xA4: R_ZP(OP_LDY());
        case 0xB4: R_ZPX(OP_LDY());
        case 0xAC: R_ABS(OP_LDY());
        case 0xBC: R_ABX(OP_LDY());
        /* LAX */
        case 0xA7: R_ZP(OP_LAX());
        case 0xB7: R_ZPY(OP_LAX());
        case 0xAF: R_ABS(OP_LAX());
        case 0xBF: R_ABY(OP_LAX());
        case 0xA3: R_IZX(OP_LAX());
        case 0xB3: R_IZY(OP_LAX());
        /* Stores */
        case 0x85: W_ZP(a);
        case 0x95: W_ZPX(a);
        case 0x8D: W_ABS(a);
        case 0x9D: W_ABX(a);
        case 0x99: W_ABY(a);
        case 0x81: W_IZX(a);
        case 0x91: W_IZY(a);
        case 0x86: W_ZP(x);
        case 0x96: W_ZPY(x);
        case 0x8E: W_ABS(x);
        case 0x84: W_ZP(y);
        case 0x94: W_ZPX(y);
        case 0x8C: W_ABS(y);
        /* SAX */
        case 0x87: W_ZP(a & x);
        case 0x97: W_ZPY(a & x);
        case 0x8F: W_ABS(a & x);
        case 0x83: W_IZX(a & x);
        /* Shifts, rotates, INC and DEC, on A and on memory */
        case 0x0A:
            v = a;
            OP_ASL();
            a = v;
            break;
        case 0x4A:
            v = a;
            OP_LSR();
            a = v;
            break;
        case 0x2A:
            v = a;
            OP_ROL();
            a = v;
            break;
        case 0x6A:
            v = a;
            OP_ROR();
            a = v;
            break;
        case 0x06: M_ZP(OP_ASL(), (void)0);
        case 0x16: M_ZPX(OP_ASL(), (void)0);
        case 0x0E: M_ABS(OP_ASL(), (void)0);
        case 0x1E: M_ABX(OP_ASL(), (void)0);
        case 0x46: M_ZP(OP_LSR(), (void)0);
        case 0x56: M_ZPX(OP_LSR(), (void)0);
        case 0x4E: M_ABS(OP_LSR(), (void)0);
        case 0x5E: M_ABX(OP_LSR(), (void)0);
        case 0x26: M_ZP(OP_ROL(), (void)0);
        case 0x36: M_ZPX(OP_ROL(), (void)0);
        case 0x2E: M_ABS(OP_ROL(), (void)0);
        case 0x3E: M_ABX(OP_ROL(), (void)0);
        case 0x66: M_ZP(OP_ROR(), (void)0);
        case 0x76: M_ZPX(OP_ROR(), (void)0);
        case 0x6E: M_ABS(OP_ROR(), (void)0);
        case 0x7E: M_ABX(OP_ROR(), (void)0);
        case 0xE6: M_ZP(OP_INC(), (void)0);
        case 0xF6: M_ZPX(OP_INC(), (void)0);
        case 0xEE: M_ABS(OP_INC(), (void)0);
        case 0xFE: M_ABX(OP_INC(), (void)0);
        case 0xC6: M_ZP(OP_DEC(), (void)0);
        case 0xD6: M_ZPX(OP_DEC(), (void)0);
        case 0xCE: M_ABS(OP_DEC(), (void)0);
        case 0xDE: M_ABX(OP_DEC(), (void)0);
/* The undocumented read-modify-writes: a shift, rotate, INC or DEC, then an operation on A
 * with the result, in seven addressing modes. */
#define RMW7(base, op, op2)                                                                        \
    case base + 0x07: M_ZP(op, op2);                                                               \
    case base + 0x17: M_ZPX(op, op2);                                                              \
    case base + 0x0F: M_ABS(op, op2);                                                              \
    case base + 0x1F: M_ABX(op, op2);                                                              \
    case base + 0x1B: M_ABY(op, op2);                                                              \
    case base + 0x03: M_IZX(op, op2);                                                              \
    case base + 0x13: M_IZY(op, op2);
            RMW7(0x00, OP_ASL(), OP_ORA()) /* SLO */
            RMW7(0x20, OP_ROL(), OP_AND()) /* RLA */
            RMW7(0x40, OP_LSR(), OP_EOR()) /* SRE */
            RMW7(0x60, OP_ROR(), OP_ADC()) /* RRA */
            RMW7(0xC0, OP_DEC(), OP_CMP(a)) /* DCP */
            RMW7(0xE0, OP_INC(), OP_SBC()) /* ISC */
        /* Transfers, increments and decrements of the registers */
        case 0xAA:
            x = a;
            NZ(x);
            break;
        case 0x8A:
            a = x;
            NZ(a);
            break;
        case 0xA8:
            y = a;
            NZ(y);
            break;
        case 0x98:
            a = y;
            NZ(a);
            break;
        case 0xBA:
            x = s;
            NZ(x);
            break;
        case 0x9A:
            s = x;
            break;
        case 0xE8:
            x = (x + 1u) & 0xFFu;
            NZ(x);
            break;
        case 0xCA:
            x = (x - 1u) & 0xFFu;
            NZ(x);
            break;
        case 0xC8:
            y = (y + 1u) & 0xFFu;
            NZ(y);
            break;
        case 0x88:
            y = (y - 1u) & 0xFFu;
            NZ(y);
            break;
        /* Flags */
        case 0x18:
            cf = 0;
            break;
        case 0x38:
            cf = 1;
            break;
        case 0x58:
            ifl = 0;
            break;
        case 0x78:
            ifl = 1;
            break;
        case 0xB8:
            vf = 0;
            break;
        case 0xD8:
            df = 0;
            break;
        case 0xF8:
            df = 1;
            break;
        /* Branches */
        case 0x10: BRANCH(!(n & 0x80u));
        case 0x30: BRANCH(n & 0x80u);
        case 0x50: BRANCH(!vf);
        case 0x70: BRANCH(vf);
        case 0x90: BRANCH(!cf);
        case 0xB0: BRANCH(cf);
        case 0xD0: BRANCH(z & 0xFFu);
        case 0xF0: BRANCH(!(z & 0xFFu));
        /* Jumps, calls, returns, the stack */
        case 0x4C:
            t = ABS16();
            pc = t;
            JUMPED();
        case 0x6C: /* the pointer's high byte from the same page */
            t = ABS16();
            pc = mem[t] | (uint32_t)mem[(t & 0xFF00u) | ((t + 1u) & 0xFFu)] << 8;
            JUMPED();
        case 0x20: /* the high byte is fetched after the pushes, as the real CPU does */
            lo = FETCH();
            PUSH(pc >> 8, 3u);
            PUSH(pc, 2u);
            pc = lo | (uint32_t)mem[pc] << 8;
            break;
        case 0x60:
            lo = PULL();
            pc = (lo | (uint32_t)PULL() << 8);
            pc = (pc + 1u) & 0xFFFFu;
            JUMPED();
        case 0x40:
            t = PULL();
            UNPACK_P(t);
            lo = PULL();
            pc = lo | (uint32_t)PULL() << 8;
            JUMPED();
        case 0x00:
            pc = (pc + 1u) & 0xFFFFu;
            PUSH(pc >> 8, 5u);
            PUSH(pc, 4u);
            PUSH(PACK_P() | 0x10u, 3u);
            ifl = 1;
            pc = mem[0xFFFE] | (uint32_t)mem[0xFFFF] << 8;
            break;
        case 0x48:
            PUSH(a, 1u);
            break;
        case 0x08:
            PUSH(PACK_P() | 0x10u, 1u);
            break;
        case 0x68:
            a = PULL();
            NZ(a);
            break;
        case 0x28:
            t = PULL();
            UNPACK_P(t);
            break;
        /* The undocumented immediates */
        case 0x0B: /* ANC */
        case 0x2B:
            v = FETCH();
            a &= v;
            NZ(a);
            cf = a >> 7;
            break;
        case 0x4B: /* ALR */
            v = FETCH();
            a &= v;
            cf = a & 1u;
            a >>= 1;
            NZ(a);
            break;
        case 0x6B: /* ARR */
            v = FETCH();
            t = a & v;
            a = (t >> 1) | cf << 7;
            if (!df) {
                NZ(a);
                cf = (a >> 6) & 1u;
                vf = ((a >> 6) ^ (a >> 5)) & 1u;
            } else {
                n = cf << 7;
                z = a;
                vf = ((t ^ a) >> 6) & 1u;
                if ((t & 0x0Fu) + (t & 0x01u) > 5u)
                    a = (a & 0xF0u) | ((a + 6u) & 0x0Fu);
                cf = (t & 0xF0u) + (t & 0x10u) > 0x50u;
                if (cf)
                    a = (a + 0x60u) & 0xFFu;
            }
            break;
        case 0xCB: /* SBX */
            v = FETCH();
            t = a & x;
            cf = t >= v;
            x = (t - v) & 0xFFu;
            NZ(x);
            break;
        case 0x8B: /* ANE, with the magic constant */
            v = FETCH();
            a = (a | 0xEEu) & x & v;
            NZ(a);
            break;
        case 0xAB: /* LXA, with the magic constant */
            v = FETCH();
            a = x = (a | 0xEEu) & v;
            NZ(a);
            break;
        case 0xBB: /* LAS */
            EA_ABY_R();
            v = RD(ea) & s;
            a = x = s = v;
            NZ(a);
            break;
        /* The unstable stores */
        case 0x93: /* SHA (zp),Y */
            t = FETCH();
            SH_STORE(mem[t] | (uint32_t)mem[(t + 1u) & 0xFFu] << 8, y, a & x);
        case 0x9F: SH_STORE(ABS16(), y, a & x); /* SHA abs,Y */
        case 0x9E: SH_STORE(ABS16(), y, x);     /* SHX */
        case 0x9C: SH_STORE(ABS16(), x, y);     /* SHY */
        case 0x9B:                              /* TAS */
            s = a & x;
            SH_STORE(ABS16(), y, a & x);
        /* NOPs, documented and not: their operands are skipped; an absolute one's is read */
        case 0xEA:
        case 0x1A:
        case 0x3A:
        case 0x5A:
        case 0x7A:
        case 0xDA:
        case 0xFA:
            break;
        case 0x80:
        case 0x82:
        case 0x89:
        case 0xC2:
        case 0xE2:
        case 0x04:
        case 0x44:
        case 0x64:
        case 0x14:
        case 0x34:
        case 0x54:
        case 0x74:
        case 0xD4:
        case 0xF4:
            pc = (pc + 1u) & 0xFFFFu;
            break;
        case 0x0C:
            EA_ABS();
            (void)RD(ea);
            break;
        case 0x1C:
        case 0x3C:
        case 0x5C:
        case 0x7C:
        case 0xDC:
        case 0xFC:
            EA_ABX_R();
            (void)RD(ea);
            break;
        /* JAM: the CPU stops, the PC on the opcode */
        default:
            pc = (pc - 1u) & 0xFFFFu;
            state = CPU6502_JAM;
            goto out;
        }
    }
out:
    c->pc = (uint16_t)pc;
    c->a = (uint8_t)a;
    c->x = (uint8_t)x;
    c->y = (uint8_t)y;
    c->s = (uint8_t)s;
    c->p = (uint8_t)PACK_P();
    c->cycles = cyc;
    c->state = (uint8_t)state;
    return state;
}

void cpu6502_reset_regs(cpu6502* c) {
    c->a = 0;
    c->x = 0;
    c->y = 0;
    c->s = 0xFF;
    c->p = 0x24;
    c->state = CPU6502_RUN;
}
