/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* A 6502 assembler for the dialect of GoatTracker 2's player.s and of the source its
 * relocator (greloc.c) generates around it: assemble(), as parse.h declares it.
 *
 * GoatTracker ships the assembler from Exomizer 2 beta by Magnus Lind, whose licence
 * allows only non-commercial use; SIDOU does not take it in (third_party/goattracker2/
 * VENDORED.md). This one was written for SIDOU from player.s, greloc.c's generated source
 * and the original's observed output; no code of the original is used. What it takes:
 * - a line holds statements: `label:`, `name = expr`, an instruction, `.ORG (expr)`,
 *   `.BYTE (expr, ...)`, `.WORD (expr, ...)`, `.IF (expr)`, `.ELSE`, `.ENDIF`; `;` starts
 *   a comment;
 * - addressing modes are written, not guessed: `#expr` immediate, `<expr` zero page (with
 *   `,x` or `,y`), `expr` absolute (with `,x` or `,y`), `(<expr,x)` and `(expr),y`
 *   indirect, nothing for implied and accumulator; branches are relative;
 * - expressions: decimal and `$` hex numbers, symbols, `.DEFINED(symbol)`, unary `-` and
 *   `!`, `* / %`, `+ -`, `< >`, `== !=`, `&&`, `||` (C's precedence), parentheses;
 * - a symbol is defined once; an assignment keeps its expression and resolves it when
 *   used; a label is the program counter; `.IF` resolves its condition at once, and the
 *   statements of a false branch are skipped unseen;
 * - output is every byte from the first statement on, in order; an operand out of range
 *   is an error (a .BYTE value out of range is written as its low byte);
 * - two quirks of the original are kept, so the bytes are the same: LDY's absolute indexed
 *   form is written `,y` and assembles as $BC (LDY abs,X), and BVS assembles as $B0;
 * - gt2ref_assemble's strict mode, for the 6502 programs SIDOU's tests write (M3's PSID
 *   fixtures), turns both quirks off (`ldy abs,x` is $BC, `ldy abs,y` an error, BVS $70) and
 *   adds BIT, BRK, CLD, CLV, RTI and SED; assemble() is never strict.
 * The gt2-ref tests check it byte for byte against GoatTracker's own assembler over every
 * example song and option set (tests/assembler.rs, with GoatTracker's release fetched
 * into build/). */
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "parse.h"

enum { T_END, T_NUM, T_SYM, T_DIR, T_OP };

typedef struct {
    int kind;
    int32_t num;
    char text[48];
} token;

typedef struct {
    token t[160];
    int n, at;
} line;

/* Expressions, kept as trees for the symbols resolved late. */
enum { E_NUM, E_SYM, E_NEG, E_NOT, E_BIN };
typedef struct expr {
    int kind;
    int op; /* E_BIN: the operator's first character, or 'A' &&, 'O' ||, 'E' ==, 'N' != */
    int32_t num;
    int sym;
    struct expr *a, *b;
} expr;

typedef struct {
    char name[48];
    expr* value;
} symbol;

/* What is emitted: a byte, or an operand resolved at the end. */
enum { A_BYTE, A_U8, A_I8, A_UI8, A_U16, A_BYTES_EXPR, A_WORD_EXPR };
typedef struct {
    int kind;
    uint8_t code;
    int32_t pc; /* after the instruction, for branches */
    expr* e;
} atom;

typedef struct {
    symbol* syms;
    int nsyms, capsyms;
    atom* atoms;
    int natoms, capatoms;
    expr** exprs;
    int nexprs, capexprs;
    int32_t pc;
    int pc_set;
    int failed;
    int strict;
    int lineno;
    const char *line_start, *line_end;
} asm_state;

static void fail(asm_state* s, const char* what, const char* detail) {
    if (!s->failed)
        fprintf(stderr, "gt2ref asm, line %d: %s %s: %.*s\n", s->lineno, what,
                detail ? detail : "", (int)(s->line_end - s->line_start), s->line_start);
    s->failed = 1;
}

static void* grow(void* p, int* cap, int need, size_t size) {
    if (need <= *cap)
        return p;
    int c = *cap ? *cap * 2 : 256;
    while (c < need)
        c *= 2;
    void* q = realloc(p, (size_t)c * size);
    if (q == NULL) {
        fprintf(stderr, "gt2ref asm: out of memory\n");
        exit(1);
    }
    *cap = c;
    return q;
}

static expr* new_expr(asm_state* s, int kind) {
    expr* e = calloc(1, sizeof *e);
    if (e == NULL) {
        fprintf(stderr, "gt2ref asm: out of memory\n");
        exit(1);
    }
    e->kind = kind;
    s->exprs = grow(s->exprs, &s->capexprs, s->nexprs + 1, sizeof *s->exprs);
    s->exprs[s->nexprs++] = e;
    return e;
}

static int find_sym(asm_state* s, const char* name) {
    for (int i = 0; i < s->nsyms; i++)
        if (strcmp(s->syms[i].name, name) == 0)
            return i;
    return -1;
}

/* A symbol's slot, made (with no value) if new. */
static int sym_ref(asm_state* s, const char* name) {
    int i = find_sym(s, name);
    if (i >= 0)
        return i;
    s->syms = grow(s->syms, &s->capsyms, s->nsyms + 1, sizeof *s->syms);
    snprintf(s->syms[s->nsyms].name, sizeof s->syms[0].name, "%s", name);
    s->syms[s->nsyms].value = NULL;
    return s->nsyms++;
}

static void define(asm_state* s, const char* name, expr* value) {
    int i = sym_ref(s, name);
    if (s->syms[i].value != NULL) {
        fail(s, "symbol defined twice:", name);
        return;
    }
    s->syms[i].value = value;
}

static int resolve(asm_state* s, expr* e, int32_t* v, int depth) {
    int32_t a = 0, b = 0;
    if (depth > 1000) {
        fail(s, "circular definition", NULL);
        return -1;
    }
    switch (e->kind) {
    case E_NUM:
        *v = e->num;
        return 0;
    case E_SYM:
        if (s->syms[e->sym].value == NULL) {
            fail(s, "undefined symbol", s->syms[e->sym].name);
            return -1;
        }
        return resolve(s, s->syms[e->sym].value, v, depth + 1);
    case E_NEG:
        if (resolve(s, e->a, &a, depth + 1))
            return -1;
        *v = -a;
        return 0;
    case E_NOT:
        if (resolve(s, e->a, &a, depth + 1))
            return -1;
        *v = !a;
        return 0;
    default:
        break;
    }
    if (resolve(s, e->a, &a, depth + 1))
        return -1;
    if (e->op == 'O' && a)
        return *v = 1, 0;
    if (e->op == 'A' && !a)
        return *v = 0, 0;
    if (resolve(s, e->b, &b, depth + 1))
        return -1;
    switch (e->op) {
    case '+': *v = a + b; break;
    case '-': *v = a - b; break;
    case '*': *v = a * b; break;
    case '/':
    case '%':
        if (b == 0) {
            fail(s, "division by zero", NULL);
            return -1;
        }
        *v = e->op == '/' ? a / b : a % b;
        break;
    case '<': *v = a < b; break;
    case '>': *v = a > b; break;
    case 'E': *v = a == b; break;
    case 'N': *v = a != b; break;
    case 'A':
    case 'O': *v = b != 0; break;
    default:
        fail(s, "bad operator", NULL);
        return -1;
    }
    return 0;
}

/* Lexing a line. */

static void lex(asm_state* s, const char* p, const char* end, line* l) {
    l->n = 0;
    l->at = 0;
    while (p < end && *p != ';') {
        if (isspace((unsigned char)*p)) {
            p++;
            continue;
        }
        if (l->n >= (int)(sizeof l->t / sizeof l->t[0]) - 1) {
            fail(s, "line too long", NULL);
            return;
        }
        token* t = &l->t[l->n++];
        memset(t, 0, sizeof *t);
        if (*p == '$' || isdigit((unsigned char)*p)) {
            int hex = *p == '$';
            if (hex)
                p++;
            t->kind = T_NUM;
            int32_t v = 0;
            while (p < end && (hex ? isxdigit((unsigned char)*p) : isdigit((unsigned char)*p))) {
                int d = isdigit((unsigned char)*p) ? *p - '0' : tolower((unsigned char)*p) - 'a' + 10;
                v = v * (hex ? 16 : 10) + d;
                p++;
            }
            t->num = v;
        } else if (isalpha((unsigned char)*p) || *p == '_' || *p == '.') {
            t->kind = *p == '.' ? T_DIR : T_SYM;
            int i = 0;
            if (*p == '.')
                p++;
            while (p < end && (isalnum((unsigned char)*p) || *p == '_')) {
                if (i < (int)sizeof t->text - 1)
                    t->text[i++] = *p;
                p++;
            }
            t->text[i] = 0;
        } else {
            t->kind = T_OP;
            t->text[0] = *p;
            if (p + 1 < end && ((p[0] == '&' && p[1] == '&') || (p[0] == '|' && p[1] == '|') ||
                                (p[0] == '=' && p[1] == '=') || (p[0] == '!' && p[1] == '='))) {
                t->text[1] = p[1];
                p++;
            }
            p++;
        }
    }
    l->t[l->n].kind = T_END;
}

static token* peek(line* l) {
    return &l->t[l->at];
}

static int is_op(line* l, const char* op) {
    token* t = peek(l);
    return t->kind == T_OP && strcmp(t->text, op) == 0;
}

static int is_dir(line* l, const char* d) {
    token* t = peek(l);
    return t->kind == T_DIR && strcasecmp(t->text, d) == 0;
}

/* Expressions: C's precedence. */

static expr* parse_expr(asm_state* s, line* l);

static expr* parse_unary(asm_state* s, line* l) {
    token* t = peek(l);
    if (is_op(l, "-") || is_op(l, "!")) {
        int neg = t->text[0] == '-';
        l->at++;
        expr* e = new_expr(s, neg ? E_NEG : E_NOT);
        e->a = parse_unary(s, l);
        return e;
    }
    if (is_op(l, "(")) {
        l->at++;
        expr* e = parse_expr(s, l);
        if (!is_op(l, ")"))
            fail(s, "missing )", NULL);
        else
            l->at++;
        return e;
    }
    if (t->kind == T_NUM) {
        l->at++;
        expr* e = new_expr(s, E_NUM);
        e->num = t->num;
        return e;
    }
    if (is_dir(l, "DEFINED")) {
        l->at++;
        expr* e = new_expr(s, E_NUM);
        if (!is_op(l, "(") || l->t[l->at + 1].kind != T_SYM || l->t[l->at + 2].kind != T_OP ||
            strcmp(l->t[l->at + 2].text, ")") != 0) {
            fail(s, "bad .DEFINED", NULL);
            return e;
        }
        int i = find_sym(s, l->t[l->at + 1].text);
        e->num = i >= 0 && s->syms[i].value != NULL;
        l->at += 3;
        return e;
    }
    if (t->kind == T_SYM) {
        l->at++;
        expr* e = new_expr(s, E_SYM);
        e->sym = sym_ref(s, t->text);
        return e;
    }
    fail(s, "expression expected at", t->text);
    l->at++;
    return new_expr(s, E_NUM);
}

static int binop(line* l, int level) {
    static const char* ops[6][3] = {
        {"||", NULL, NULL}, {"&&", NULL, NULL}, {"==", "!=", NULL},
        {"<", ">", NULL},   {"+", "-", NULL},   {"*", "/", "%"}};
    token* t = peek(l);
    if (t->kind != T_OP)
        return 0;
    for (int i = 0; i < 3 && ops[level][i]; i++) {
        if (strcmp(t->text, ops[level][i]) == 0) {
            const char* o = ops[level][i];
            if (strcmp(o, "||") == 0)
                return 'O';
            if (strcmp(o, "&&") == 0)
                return 'A';
            if (strcmp(o, "==") == 0)
                return 'E';
            if (strcmp(o, "!=") == 0)
                return 'N';
            return o[0];
        }
    }
    return 0;
}

static expr* parse_level(asm_state* s, line* l, int level) {
    if (level == 6)
        return parse_unary(s, l);
    expr* e = parse_level(s, l, level + 1);
    int op;
    while ((op = binop(l, level)) != 0) {
        l->at++;
        expr* b = new_expr(s, E_BIN);
        b->op = op;
        b->a = e;
        b->b = parse_level(s, l, level + 1);
        e = b;
    }
    return e;
}

static expr* parse_expr(asm_state* s, line* l) {
    return parse_level(s, l, 0);
}

/* Instructions: the mnemonics the dialect has, with an opcode for each mode (0 none). */

enum { M_IMP, M_IMM, M_ZP, M_ZPX, M_ZPY, M_ABS, M_ABSX, M_ABSY, M_INDX, M_INDY, M_REL, M_N };

typedef struct {
    const char* name;
    int16_t op[M_N];
} mnemonic;

#define NO -1
static const mnemonic MNEMONICS[] = {
    /*        IMP   IMM   ZP    ZPX   ZPY   ABS   ABSX  ABSY  INDX  INDY  REL */
    {"lda", {NO, 0xa9, 0xa5, 0xb5, NO, 0xad, 0xbd, 0xb9, 0xa1, 0xb1, NO}},
    {"ldx", {NO, 0xa2, 0xa6, NO, 0xb6, 0xae, NO, 0xbe, NO, NO, NO}},
    /* The dialect writes LDY's absolute indexed form as `,y` and makes it $BC, LDY abs,X;
     * player.s relies on it (`ldy mt_chnnote,y`, X the channel). */
    {"ldy", {NO, 0xa0, 0xa4, 0xb4, NO, 0xac, NO, 0xbc, NO, NO, NO}},
    {"sta", {NO, NO, 0x85, 0x95, NO, 0x8d, 0x9d, 0x99, 0x81, 0x91, NO}},
    {"stx", {NO, NO, 0x86, NO, 0x96, 0x8e, NO, NO, NO, NO, NO}},
    {"sty", {NO, NO, 0x84, 0x94, NO, 0x8c, NO, NO, NO, NO, NO}},
    {"and", {NO, 0x29, 0x25, 0x35, NO, 0x2d, 0x3d, 0x39, 0x21, 0x31, NO}},
    {"ora", {NO, 0x09, 0x05, 0x15, NO, 0x0d, 0x1d, 0x19, 0x01, 0x11, NO}},
    {"eor", {NO, 0x49, 0x45, 0x55, NO, 0x4d, 0x5d, 0x59, 0x41, 0x51, NO}},
    {"adc", {NO, 0x69, 0x65, 0x75, NO, 0x6d, 0x7d, 0x79, 0x61, 0x71, NO}},
    {"sbc", {NO, 0xe9, 0xe5, 0xf5, NO, 0xed, 0xfd, 0xf9, 0xe1, 0xf1, NO}},
    {"cmp", {NO, 0xc9, 0xc5, 0xd5, NO, 0xcd, 0xdd, 0xd9, 0xc1, 0xd1, NO}},
    {"cpx", {NO, 0xe0, 0xe4, NO, NO, 0xec, NO, NO, NO, NO, NO}},
    {"cpy", {NO, 0xc0, 0xc4, NO, NO, 0xcc, NO, NO, NO, NO, NO}},
    {"inc", {NO, NO, 0xe6, 0xf6, NO, 0xee, 0xfe, NO, NO, NO, NO}},
    {"dec", {NO, NO, 0xc6, 0xd6, NO, 0xce, 0xde, NO, NO, NO, NO}},
    {"lsr", {0x4a, NO, 0x46, 0x56, NO, 0x4e, 0x5e, NO, NO, NO, NO}},
    {"asl", {0x0a, NO, 0x06, 0x16, NO, 0x0e, 0x1e, NO, NO, NO, NO}},
    {"ror", {0x6a, NO, 0x66, 0x76, NO, 0x6e, 0x7e, NO, NO, NO, NO}},
    {"rol", {0x2a, NO, 0x26, 0x36, NO, 0x2e, 0x3e, NO, NO, NO, NO}},
    {"jmp", {NO, NO, NO, NO, NO, 0x4c, NO, NO, NO, NO, NO}},
    {"jsr", {NO, NO, NO, NO, NO, 0x20, NO, NO, NO, NO, NO}},
    {"beq", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0xf0}},
    {"bne", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0xd0}},
    {"bcc", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0x90}},
    {"bcs", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0xb0}},
    {"bpl", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0x10}},
    {"bmi", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0x30}},
    {"bvc", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0x50}},
    /* The dialect's BVS is $B0 (BCS); player.s never uses it. */
    {"bvs", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0xb0}},
    {"tsx", {0xba, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"txs", {0x9a, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"pha", {0x48, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"pla", {0x68, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"php", {0x08, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"plp", {0x28, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"sei", {0x78, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"cli", {0x58, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"nop", {0xea, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"tya", {0x98, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"tay", {0xa8, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"txa", {0x8a, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"tax", {0xaa, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"clc", {0x18, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"sec", {0x38, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"rts", {0x60, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"inx", {0xe8, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"dex", {0xca, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"iny", {0xc8, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"dey", {0x88, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
};

/* Strict mode's table, looked up before the dialect's: the quirks off, and what player.s
 * never needed. */
static const mnemonic STRICT_MNEMONICS[] = {
    /*        IMP   IMM   ZP    ZPX   ZPY   ABS   ABSX  ABSY  INDX  INDY  REL */
    {"ldy", {NO, 0xa0, 0xa4, 0xb4, NO, 0xac, 0xbc, NO, NO, NO, NO}},
    {"bvs", {NO, NO, NO, NO, NO, NO, NO, NO, NO, NO, 0x70}},
    {"bit", {NO, NO, 0x24, NO, NO, 0x2c, NO, NO, NO, NO, NO}},
    {"brk", {0x00, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"cld", {0xd8, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"clv", {0xb8, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"rti", {0x40, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
    {"sed", {0xf8, NO, NO, NO, NO, NO, NO, NO, NO, NO, NO}},
};

static const mnemonic* find_mnemonic(const asm_state* s, const char* name) {
    if (s->strict)
        for (size_t i = 0; i < sizeof STRICT_MNEMONICS / sizeof STRICT_MNEMONICS[0]; i++)
            if (strcasecmp(STRICT_MNEMONICS[i].name, name) == 0)
                return &STRICT_MNEMONICS[i];
    for (size_t i = 0; i < sizeof MNEMONICS / sizeof MNEMONICS[0]; i++)
        if (strcasecmp(MNEMONICS[i].name, name) == 0)
            return &MNEMONICS[i];
    return NULL;
}

static void emit(asm_state* s, int kind, int code, expr* e, int size) {
    s->atoms = grow(s->atoms, &s->capatoms, s->natoms + 1, sizeof *s->atoms);
    atom* a = &s->atoms[s->natoms++];
    a->kind = kind;
    a->code = (uint8_t)code;
    a->e = e;
    if (!s->pc_set)
        fail(s, "no .ORG before code", NULL);
    s->pc += size;
    a->pc = s->pc;
}

static int is_reg(line* l, int at, char r) {
    token* t = &l->t[at];
    return t->kind == T_SYM && t->text[1] == 0 && tolower((unsigned char)t->text[0]) == r;
}

static void instruction(asm_state* s, line* l, const mnemonic* m) {
    int mode;
    expr* e = NULL;
    token* t = peek(l);
    if (t->kind == T_END || t->kind == T_DIR) {
        mode = M_IMP;
    } else if (is_op(l, "#")) {
        l->at++;
        e = parse_expr(s, l);
        mode = M_IMM;
    } else if (is_op(l, "<")) {
        l->at++;
        e = parse_expr(s, l);
        mode = M_ZP;
        if (is_op(l, ",")) {
            mode = is_reg(l, l->at + 1, 'x') ? M_ZPX : is_reg(l, l->at + 1, 'y') ? M_ZPY : -1;
            l->at += 2;
        }
    } else if (is_op(l, "(") && m->op[M_INDY] != NO) {
        /* (<expr,x) or (expr),y; anything else in parentheses is an absolute expression */
        int save = l->at;
        l->at++;
        int zp = is_op(l, "<");
        if (zp)
            l->at++;
        e = parse_expr(s, l);
        if (is_op(l, ",") && is_reg(l, l->at + 1, 'x') && l->t[l->at + 2].kind == T_OP &&
            strcmp(l->t[l->at + 2].text, ")") == 0) {
            l->at += 3;
            mode = M_INDX;
        } else if (is_op(l, ")") && l->t[l->at + 1].kind == T_OP &&
                   strcmp(l->t[l->at + 1].text, ",") == 0 && is_reg(l, l->at + 2, 'y')) {
            l->at += 3;
            mode = M_INDY;
        } else {
            l->at = save;
            e = parse_expr(s, l);
            mode = M_ABS;
        }
    } else {
        e = parse_expr(s, l);
        mode = m->op[M_REL] != NO ? M_REL : M_ABS;
        if (is_op(l, ",")) {
            mode = is_reg(l, l->at + 1, 'x') ? M_ABSX : is_reg(l, l->at + 1, 'y') ? M_ABSY : -1;
            l->at += 2;
        }
    }
    if (mode < 0 || m->op[mode] == NO) {
        fail(s, "addressing mode not allowed for", m->name);
        return;
    }
    int code = m->op[mode];
    switch (mode) {
    case M_IMP: emit(s, A_BYTE, code, NULL, 1); break;
    case M_IMM: emit(s, A_UI8, code, e, 2); break;
    case M_REL: emit(s, A_I8, code, e, 2); break;
    case M_ABS:
    case M_ABSX:
    case M_ABSY: emit(s, A_U16, code, e, 3); break;
    default: emit(s, A_U8, code, e, 2); break;
    }
}

/* .BYTE (list) and .WORD (list) */
static void data(asm_state* s, line* l, int words) {
    if (!is_op(l, "(")) {
        fail(s, "( expected after", words ? ".WORD" : ".BYTE");
        return;
    }
    l->at++;
    for (;;) {
        expr* e = parse_expr(s, l);
        emit(s, words ? A_WORD_EXPR : A_BYTES_EXPR, 0, e, words ? 2 : 1);
        if (is_op(l, ",")) {
            l->at++;
            continue;
        }
        break;
    }
    if (!is_op(l, ")"))
        fail(s, ") expected", NULL);
    else
        l->at++;
}

/* The .IF stack: each level's state. */
enum { IF_TRUE, IF_FALSE, IF_DEAD }; /* DEAD: inside a skipped outer branch */

static int active(const int* stack, int depth) {
    return depth == 0 || stack[depth - 1] == IF_TRUE;
}

static int32_t resolve_now(asm_state* s, expr* e) {
    int32_t v = 0;
    if (resolve(s, e, &v, 0))
        return 0;
    return v;
}

static int assemble_mode(struct membuf* src, struct membuf* dest, int strict) {
    asm_state st;
    asm_state* s = &st;
    memset(s, 0, sizeof *s);
    s->strict = strict;
    int ifs[256], depth = 0;
    const char* p = (const char*)membuf_get(src);
    const char* end = p + membuf_memlen(src);
    line l;
    while (p < end && !s->failed) {
        const char* eol = p;
        while (eol < end && *eol != '\n')
            eol++;
        s->lineno++;
        s->line_start = p;
        s->line_end = eol;
        lex(s, p, eol, &l);
        p = eol < end ? eol + 1 : eol;
        while (peek(&l)->kind != T_END && !s->failed) {
            token* t = peek(&l);
            if (t->kind == T_DIR && strcasecmp(t->text, "IF") == 0) {
                l.at++;
                if (depth >= 256) {
                    fail(s, ".IF nested too deep", NULL);
                    break;
                }
                if (!active(ifs, depth)) {
                    /* skip the condition unseen */
                    int nest = 0;
                    while (peek(&l)->kind != T_END) {
                        if (is_op(&l, "("))
                            nest++;
                        if (is_op(&l, ")") && --nest == 0) {
                            l.at++;
                            break;
                        }
                        l.at++;
                    }
                    ifs[depth++] = IF_DEAD;
                } else {
                    ifs[depth++] = resolve_now(s, parse_expr(s, &l)) ? IF_TRUE : IF_FALSE;
                }
                continue;
            }
            if (t->kind == T_DIR && strcasecmp(t->text, "ELSE") == 0) {
                l.at++;
                if (depth == 0)
                    fail(s, ".ELSE without .IF", NULL);
                else if (ifs[depth - 1] != IF_DEAD)
                    ifs[depth - 1] = ifs[depth - 1] == IF_TRUE ? IF_FALSE : IF_TRUE;
                continue;
            }
            if (t->kind == T_DIR && strcasecmp(t->text, "ENDIF") == 0) {
                l.at++;
                if (depth == 0)
                    fail(s, ".ENDIF without .IF", NULL);
                else
                    depth--;
                continue;
            }
            if (!active(ifs, depth)) {
                /* a skipped statement: its tokens up to the next directive of the .IF kind */
                l.at++;
                while (peek(&l)->kind != T_END &&
                       !(peek(&l)->kind == T_DIR && (strcasecmp(peek(&l)->text, "IF") == 0 ||
                                                    strcasecmp(peek(&l)->text, "ELSE") == 0 ||
                                                    strcasecmp(peek(&l)->text, "ENDIF") == 0)))
                    l.at++;
                continue;
            }
            if (t->kind == T_DIR) {
                l.at++;
                if (strcasecmp(t->text, "ORG") == 0) {
                    s->pc = resolve_now(s, parse_expr(s, &l));
                    s->pc_set = 1;
                } else if (strcasecmp(t->text, "BYTE") == 0) {
                    data(s, &l, 0);
                } else if (strcasecmp(t->text, "WORD") == 0) {
                    data(s, &l, 1);
                } else {
                    fail(s, "unknown directive", t->text);
                }
                continue;
            }
            if (t->kind == T_SYM && l.t[l.at + 1].kind == T_OP &&
                strcmp(l.t[l.at + 1].text, ":") == 0) {
                expr* e = new_expr(s, E_NUM);
                e->num = s->pc;
                if (!s->pc_set)
                    fail(s, "label before .ORG:", t->text);
                define(s, t->text, e);
                l.at += 2;
                continue;
            }
            if (t->kind == T_SYM && l.t[l.at + 1].kind == T_OP &&
                strcmp(l.t[l.at + 1].text, "=") == 0) {
                char name[48];
                snprintf(name, sizeof name, "%s", t->text);
                l.at += 2;
                define(s, name, parse_expr(s, &l));
                continue;
            }
            if (t->kind == T_SYM) {
                const mnemonic* m = find_mnemonic(s, t->text);
                if (m) {
                    l.at++;
                    instruction(s, &l, m);
                    continue;
                }
            }
            fail(s, "syntax error at", t->text);
        }
    }
    if (depth != 0)
        fail(s, ".IF without .ENDIF", NULL);
    /* Output. */
    for (int i = 0; i < s->natoms && !s->failed; i++) {
        atom* a = &s->atoms[i];
        int32_t v = 0;
        if (a->e && resolve(s, a->e, &v, 0))
            break;
        unsigned char b[3] = {a->code, 0, 0};
        switch (a->kind) {
        case A_BYTE:
            membuf_append(dest, b, 1);
            break;
        case A_U8:
        case A_UI8:
        case A_I8:
            if (a->kind == A_I8)
                v -= a->pc;
            if ((a->kind == A_U8 && (v < 0 || v > 255)) ||
                (a->kind == A_UI8 && (v < -128 || v > 255)) ||
                (a->kind == A_I8 && (v < -128 || v > 127))) {
                fail(s, "operand out of range", NULL);
                break;
            }
            b[1] = (unsigned char)v;
            membuf_append(dest, b, 2);
            break;
        case A_U16:
            if (v < 0 || v > 65535) {
                fail(s, "operand out of range", NULL);
                break;
            }
            b[1] = (unsigned char)(v % 256);
            b[2] = (unsigned char)(v / 256);
            membuf_append(dest, b, 3);
            break;
        case A_BYTES_EXPR:
            if (v < -128 || v > 255)
                fprintf(stderr, "gt2ref asm: .BYTE value %d out of range\n", (int)v);
            b[0] = (unsigned char)v;
            membuf_append(dest, b, 1);
            break;
        default: /* A_WORD_EXPR */
            b[0] = (unsigned char)(v % 256);
            b[1] = (unsigned char)(v / 256);
            membuf_append(dest, b, 2);
            break;
        }
    }
    int failed = s->failed;
    for (int i = 0; i < s->nexprs; i++)
        free(s->exprs[i]);
    free(s->exprs);
    free(s->syms);
    free(s->atoms);
    return failed;
}

int assemble(struct membuf* src, struct membuf* dest) {
    return assemble_mode(src, dest, 0);
}

/* The source text src[0..len), assembled (strict or the dialect) into a malloc'd buffer the
 * caller frees with gt2ref_free: 0, or nonzero on an error (printed to stderr). */
int gt2ref_assemble(const unsigned char* text, size_t len, int strict, unsigned char** out,
                    size_t* out_len) {
    struct membuf src = STATIC_MEMBUF_INIT, dest = STATIC_MEMBUF_INIT;
    membuf_append(&src, text, (int)len);
    int r = assemble_mode(&src, &dest, strict);
    *out_len = (size_t)membuf_memlen(&dest);
    *out = malloc(*out_len ? *out_len : 1);
    if (*out == NULL) {
        fprintf(stderr, "gt2ref asm: out of memory\n");
        exit(1);
    }
    if (*out_len)
        memcpy(*out, membuf_get(&dest), *out_len);
    membuf_free(&src);
    membuf_free(&dest);
    return r;
}
