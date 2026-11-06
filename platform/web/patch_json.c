// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "patch_json.h"

#include <string.h>

#include "lab_edit.h"
#include "lab_song.h"
#include "live.h"
#include "sid/sid_filter.h"

enum {
    K_INT,
    K_SCALED,
    K_BOOL,
    K_ENUM,
    K_SET,
    K_VOICES,
    K_HZ,
    K_RATE,
    K_NOTE,
    K_NAME,
    K_FORMAT,
    K_GROUP,
    K_ALSO,
    K_LIST
};

enum { F_TUNING = 1, F_WIDE = 2, F_ALWAYS = 4, F_PLUS1 = 8 };
enum { L_LAYERS, L_STEPS, L_LFOS };

typedef struct {
    uint32_t num, den, max;
} scale;

typedef struct field field;

typedef struct {
    const field* sub;
    uint8_t first, stride, tuning, tuning_stride, max, kind;
} list;

struct field {
    const char* name;
    uint8_t kind, at, mask, flags;
    int16_t lo, hi;
    const void* more;
};

#define MILLI 1000
#define CUTOFF_MODEL SID_MODEL_8580
#define VOICE_BYTES LAB_SOUND_LAYER_VOICE_BYTES
#define CONTROL_KEPT 0xFEu
#define NAME_FIRST ' '
#define NAME_LAST '_'
#define NOTE_DOWN_MIN (-32)
#define NOTE_HOLD LAB_TABLE_NOTE_NONE

static const char WAVES[] = "none\0triangle\0saw\0pulse\0noise\0";
static const char MODES[] = "off\0low\0band\0high\0";
static const char VELOCITY[] = "off\0sustain\0cutoff\0pulse_width\0";
static const char MOD_WHEEL[] = "pulse_width\0vibrato\0cutoff\0";
static const char AFTERTOUCH[] = "off\0vibrato\0pulse_width\0cutoff\0";
static const char ARP_RATES[] = "1fr\0002fr\0003fr\0004fr\0006fr\0008fr\0001/32\0001/16\0001/8\0";
static const char ARP_ORDERS[] = "up\0down\0up_down\0played\0";
static const char LFO_SHAPES[] = "off\0triangle\0saw_up\0saw_down\0square\0sample_hold\0";
static const char LFO_DESTS[] = "pitch\0pulse_width\0cutoff\0volume\0";
static const char DISTORTIONS[] = "off\0tape\0overdrive\0fuzz\0crush\0lofi\0";
static const char MODULATIONS[] = "off\0chorus\0flanger\0phaser\0tremolo\0autopan\0wow\0";
static const char EFFECTS[] = "off\0delay\0pingpong\0reverb\0lush\0";
static const char NOTE_NAMES[] = "C\0C#\0D\0D#\0E\0F\0F#\0G\0G#\0A\0A#\0B\0";

static const scale PULSE = {LIVE_PULSE_WIDTH_MAX, 100u, LIVE_PULSE_WIDTH_MAX};
static const scale PWM_DEPTH = {4096u, 100u, LIVE_PWM_DEPTH_MAX};
static const scale PWM_RATE = {65536u, LIVE_TICK_HZ, LIVE_PWM_STEP_MAX};
static const scale SYNC_RATIO = {256u, 1u, LAB_SOUND_SYNC_BASE_MAX};
static const scale VIBRATO_RATE = {10u, 1u, LIVE_EXPRESSION_MAX};

static const field ENVELOPE[] = {
    {"attack", K_INT, 1, 0xF0, 0, 0, 15, 0},
    {"decay", K_INT, 1, 0x0F, 0, 0, 15, 0},
    {"sustain", K_INT, 2, 0xF0, 0, 0, 15, 0},
    {"release", K_INT, 2, 0x0F, 0, 0, 15, 0},
    {0},
};

static const field VOICE[] = {
    {"wave", K_SET, 0, 0xF0, F_ALWAYS, 0, 0, WAVES},
    {"ring", K_BOOL, 0, 0x04, 0, 0, 0, 0},
    {"sync", K_BOOL, 0, 0x02, 0, 0, 0, 0},
    {"pulse_width", K_SCALED, 3, 0xFF, F_WIDE, 0, 0, &PULSE},
    {"envelope", K_GROUP, 0, 0, F_ALWAYS, 0, 0, ENVELOPE},
    {"transpose", K_INT, 0, 0xFF, F_TUNING, -LIVE_TRANSPOSE_MAX, LIVE_TRANSPOSE_MAX, 0},
    {"detune", K_INT, 1, 0xFF, F_TUNING, -LIVE_DETUNE_CENTS, LIVE_DETUNE_CENTS, 0},
    {0},
};

static const field FILTER_ENVELOPE[] = {
    {"depth", K_INT, 0, 0xFF, 0, -LIVE_FENV_DEPTH_MAX, LIVE_FENV_DEPTH_MAX, 0},
    {"", K_ALSO, 0, 0, 0, 0, 0, ENVELOPE},
    {0},
};

static const field FILTER[] = {
    {"mode", K_SET, LAB_SOUND_FILTER_MODE, 0x70, 0, 0, 0, MODES},
    {"cutoff_hz", K_HZ, LAB_SOUND_CUTOFF, 0xFF, F_WIDE, 0, 0, 0},
    {"resonance", K_INT, LAB_SOUND_RESONANCE, 0xFF, 0, 0, LIVE_RESONANCE_MAX, 0},
    {"voices", K_VOICES, LAB_SOUND_FILTER_VOICES, 0x07, 0, 1, LIVE_VOICES, 0},
    {"resonance_link", K_INT, LAB_SOUND_RESO_LINK, 0xFF, 0, -LIVE_LINK_MAX, LIVE_LINK_MAX, 0},
    {"lfo_depth", K_INT, LAB_SOUND_FILTER_LFO, 0xFF, 0, 0, LIVE_FILTER_LFO_MAX, 0},
    {"envelope", K_GROUP, LAB_SOUND_FILTER_ENV, 0, 0, 0, 0, FILTER_ENVELOPE},
    {0},
};

static const field PWM[] = {
    {"depth", K_SCALED, LAB_SOUND_PWM_DEPTH, 0xFF, F_WIDE, 0, 0, &PWM_DEPTH},
    {"rate_hz", K_SCALED, LAB_SOUND_PWM_RATE, 0xFF, F_WIDE, 0, 0, &PWM_RATE},
    {"link", K_INT, LAB_SOUND_PWM_LINK, 0xFF, 0, -LIVE_LINK_MAX, LIVE_LINK_MAX, 0},
    {0},
};

static const field SYNC_LEAD[] = {
    {"on", K_BOOL, LAB_SOUND_MONO_SYNC, 0x01, 0, 0, 0, 0},
    {"ratio", K_SCALED, LAB_SOUND_SYNC_BASE, 0xFF, F_WIDE, 0, 0, &SYNC_RATIO},
    {"semitones", K_INT, LAB_SOUND_SYNC_SEMIS, 0xFF, 0, -LIVE_RATIO_SEMIS, LIVE_RATIO_SEMIS, 0},
    {0},
};

static const field VIBRATO[] = {
    {"rate_hz", K_SCALED, LAB_SOUND_VIBRATO_RATE, 0xFF, 0, 0, 0, &VIBRATO_RATE},
    {"depth", K_INT, LAB_SOUND_VIBRATO_DEPTH, 0xFF, 0, 0, LIVE_EXPRESSION_MAX, 0},
    {"delay", K_INT, LAB_SOUND_VIBRATO_DELAY, 0xFF, 0, 0, LIVE_EXPRESSION_MAX, 0},
    {0},
};

static const field ARP[] = {
    {"on", K_BOOL, LAB_SOUND_ARP + LIVE_ARP_ON, 0x01, 0, 0, 0, 0},
    {"rate", K_ENUM, LAB_SOUND_ARP + LIVE_ARP_RATE, 0xFF, 0, 0, 0, ARP_RATES},
    {"order", K_ENUM, LAB_SOUND_ARP + LIVE_ARP_ORDER, 0xFF, 0, 0, 0, ARP_ORDERS},
    {"octaves", K_INT, LAB_SOUND_ARP + LIVE_ARP_SPAN, 0xFF, 0, 1, LIVE_ARP_SPAN_MAX, 0},
    {0},
};

static const field SLOT_KNOBS[] = {
    {"amount", K_INT, LIVE_SLOT_AMOUNT, 0xFF, 0, 0, LIVE_SLOT_MAX, 0},
    {"a", K_INT, LIVE_SLOT_A, 0xFF, 0, 0, LIVE_SLOT_MAX, 0},
    {"b", K_INT, LIVE_SLOT_B, 0xFF, 0, 0, LIVE_SLOT_MAX, 0},
    {"mix", K_INT, LIVE_SLOT_MIX, 0xFF, 0, 0, LIVE_SLOT_MAX, 0},
    {0},
};

static const field DISTORTION[] = {
    {"type", K_ENUM, LIVE_SLOT_ALGORITHM, 0xFF, 0, 0, 0, DISTORTIONS},
    {"", K_ALSO, 0, 0, 0, 0, 0, SLOT_KNOBS},
    {0},
};
static const field MODULATION[] = {
    {"type", K_ENUM, LIVE_SLOT_ALGORITHM, 0xFF, 0, 0, 0, MODULATIONS},
    {"", K_ALSO, 0, 0, 0, 0, 0, SLOT_KNOBS},
    {0},
};
static const field EFFECT[] = {
    {"type", K_ENUM, LIVE_SLOT_ALGORITHM, 0xFF, 0, 0, 0, EFFECTS},
    {"", K_ALSO, 0, 0, 0, 0, 0, SLOT_KNOBS},
    {0},
};

static const field FX[] = {
    {"distortion", K_GROUP, LAB_SOUND_SLOTS, 0, 0, 0, 0, DISTORTION},
    {"modulation", K_GROUP, LAB_SOUND_SLOTS + LIVE_SLOT_FIELDS, 0, 0, 0, 0, MODULATION},
    {"effect", K_GROUP, LAB_SOUND_SLOTS + 2u * LIVE_SLOT_FIELDS, 0, 0, 0, 0, EFFECT},
    {0},
};

static const field STEP[] = {
    {"wave", K_SET, 0, 0xF0, 0, 0, 0, WAVES},
    {"test", K_BOOL, 0, 0x08, 0, 0, 0, 0},
    {"ring", K_BOOL, 0, 0x04, 0, 0, 0, 0},
    {"sync", K_BOOL, 0, 0x02, 0, 0, 0, 0},
    {"note", K_NOTE, 1, 0xFF, 0, NOTE_DOWN_MIN, LAB_TABLE_NOTE_UP_MAX, 0},
    {0},
};

static const list STEPS = {STEP, LAB_SOUND_TABLE_STEPS, 2u, 0, 0, LAB_TABLE_STEPS_MAX, L_STEPS};

static const field TABLE[] = {
    {"on", K_BOOL, LAB_SOUND_TABLE, LAB_TABLE_ON, 0, 0, 0, 0},
    {"layer", K_INT, LAB_SOUND_TABLE, LAB_TABLE_LAYER_MASK, F_PLUS1, 1, LIVE_LAYERS_MAX + 1, 0},
    {"loop", K_INT, LAB_SOUND_TABLE_LOOP, 0xFF, 0, 0, LAB_TABLE_STEPS_MAX, 0},
    {"frames", K_INT, LAB_SOUND_TABLE_RATE, 0xFF, F_PLUS1, 1, LAB_TABLE_RATE_MAX, 0},
    {"steps", K_LIST, 0, 0, 0, 0, 0, &STEPS},
    {0},
};

static const field LFO[] = {
    {"shape", K_ENUM, 0, LAB_LFO_SHAPE_MASK, 0, 0, 0, LFO_SHAPES},
    {"to", K_ENUM, 0, LAB_LFO_DEST_MASK, 0, 0, 0, LFO_DESTS},
    {"rate_hz", K_RATE, 1, 0xFF, 0, 0, 0, 0},
    {"depth", K_INT, 2, 0xFF, 0, -LAB_LFO_DEPTH_MAX, LAB_LFO_DEPTH_MAX, 0},
    {0},
};

static const list LAYERS = {VOICE,
                            LAB_SOUND_LAYER_VOICE,
                            VOICE_BYTES,
                            LAB_SOUND_LAYER_TUNING + LAB_SOUND_LAYER_TUNING_BYTES,
                            LAB_SOUND_LAYER_TUNING_BYTES,
                            LIVE_LAYERS_MAX,
                            L_LAYERS};
static const list LFOS = {LFO, LAB_SOUND_LFO, LAB_SOUND_LFO_BYTES, 0, 0, LAB_SOUND_LFOS, L_LFOS};

static const field ROOT[] = {
    {"format", K_FORMAT, 0, 0, F_ALWAYS, 0, 0, 0},
    {"name", K_NAME, 0, 0, F_ALWAYS, 0, 0, 0},
    {"", K_ALSO, 0, 0, 0, 0, 0, VOICE},
    {"fine_tune", K_INT, LAB_SOUND_TUNE, 0xFF, 0, -LIVE_TUNE_CENTS, LIVE_TUNE_CENTS, 0},
    {"filter", K_GROUP, 0, 0, 0, 0, 0, FILTER},
    {"pwm", K_GROUP, 0, 0, 0, 0, 0, PWM},
    {"sync_lead", K_GROUP, 0, 0, 0, 0, 0, SYNC_LEAD},
    {"vibrato", K_GROUP, 0, 0, 0, 0, 0, VIBRATO},
    {"arp", K_GROUP, 0, 0, 0, 0, 0, ARP},
    {"bend_range", K_INT, LAB_SOUND_BEND_RANGE, 0xFF, 0, 0, LIVE_BEND_RANGE_MAX, 0},
    {"velocity", K_ENUM, LAB_SOUND_VELOCITY, 0xFF, 0, 0, 0, VELOCITY},
    {"mod_wheel", K_ENUM, LAB_SOUND_MOD, 0xFF, 0, 0, 0, MOD_WHEEL},
    {"aftertouch", K_ENUM, LAB_SOUND_PRESSURE, 0xFF, 0, 0, 0, AFTERTOUCH},
    {"layers", K_LIST, 0, 0, 0, 0, 0, &LAYERS},
    {"table", K_GROUP, 0, 0, 0, 0, 0, TABLE},
    {"lfos", K_LIST, 0, 0, 0, 0, 0, &LFOS},
    {"fx", K_GROUP, 0, 0, 0, 0, 0, FX},
    {0},
};

static const uint8_t DEFAULTS[LAB_SOUND_BYTES] = {
    [LAB_SOUND_CONTROL] = 0x20,
    [LAB_SOUND_AD] = 0x09,
    [LAB_SOUND_SR] = 0xa6,
    [LAB_SOUND_PULSE + 1u] = 0x08,
    [LAB_SOUND_CUTOFF] = 1200u & 0xFFu,
    [LAB_SOUND_CUTOFF + 1u] = 1200u >> 8,
    [LAB_SOUND_RESONANCE] = 6,
    [LAB_SOUND_FILTER_VOICES] = 0x07,
    [LAB_SOUND_FILTER_MODE] = 0x10,
    [LAB_SOUND_BEND_RANGE] = LIVE_BEND_RANGE_DEFAULT,
    [LAB_SOUND_VIBRATO_RATE] = LIVE_VIBRATO_RATE_DEFAULT,
    [LAB_SOUND_ARP + LIVE_ARP_RATE] = LIVE_ARP_RATE_DEFAULT,
    [LAB_SOUND_ARP + LIVE_ARP_SPAN] = 1,
};

void patch_json_defaults(uint8_t sound[LAB_SOUND_BYTES]) {
    memcpy(sound, DEFAULTS, LAB_SOUND_BYTES);
}

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

static const char* nth(const char* names, uint32_t i) {
    for (; *names; names += length(names) + 1u) {
        if (!i--) {
            return names;
        }
    }
    return NULL;
}

static uint32_t shift_of(uint32_t mask) {
    uint32_t s = 0;
    while (mask && !(mask >> s & 1u)) {
        s++;
    }
    return s;
}

static uint32_t at_of(const field* f, uint32_t a, uint32_t b) {
    return (f->flags & F_TUNING ? b : a) + f->at;
}

static uint32_t get(const uint8_t* s, const field* f, uint32_t a, uint32_t b) {
    uint32_t at = at_of(f, a, b);
    if (f->flags & F_WIDE) {
        return (uint32_t)s[at] | (uint32_t)s[at + 1u] << 8;
    }
    return (s[at] & f->mask) >> shift_of(f->mask);
}

static void mark(uint8_t* s, uint8_t* touched, uint32_t at, uint32_t mask, uint32_t bits) {
    s[at] = (uint8_t)((s[at] & ~mask) | (bits & mask));
    touched[at] |= (uint8_t)mask;
}

static void put(uint8_t* s, uint8_t* touched, const field* f, uint32_t a, uint32_t b, uint32_t v) {
    uint32_t at = at_of(f, a, b);
    if (f->flags & F_WIDE) {
        mark(s, touched, at, 0xFFu, v);
        mark(s, touched, at + 1u, 0xFFu, v >> 8);
    } else {
        mark(s, touched, at, f->mask, v << shift_of(f->mask));
    }
}

static int64_t scaled_reg(const scale* k, int64_t milli) {
    return (milli * k->num * 2 + (int64_t)k->den * MILLI) / ((int64_t)k->den * 2 * MILLI);
}

static float cutoff_hz(uint32_t fc) {
    return sid_cutoff_hz(CUTOFF_MODEL, 0, fc);
}

static uint32_t cutoff_max_hz(void) {
    return (uint32_t)(cutoff_hz(LIVE_CUTOFF_MAX) + 0.5f);
}

static uint32_t cutoff_of(int64_t milli) {
    float hz = (float)milli / (float)MILLI;
    uint32_t lo = 0, hi = LIVE_CUTOFF_MAX;
    while (lo < hi) {
        uint32_t mid = (lo + hi) / 2u;
        if (cutoff_hz(mid) < hz) {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }
    return lo && hz - cutoff_hz(lo - 1u) <= cutoff_hz(lo) - hz ? lo - 1u : lo;
}

static int64_t rate_milli(uint32_t detent) {
    return (int64_t)live_walk_at(LIVE_WALK_PWM_RATE, detent) * LIVE_TICK_HZ * MILLI / 65536;
}

static uint32_t rate_of(int64_t milli) {
    uint32_t best = 0;
    int64_t gap = -1;
    for (uint32_t i = 0; i < LAB_LFO_RATES; i++) {
        int64_t d = (int64_t)live_walk_at(LIVE_WALK_PWM_RATE, i) * LIVE_TICK_HZ * MILLI - milli * 65536;
        d = d < 0 ? -d : d;
        if (gap < 0 || d < gap) {
            gap = d;
            best = i;
        }
    }
    return best;
}

typedef struct {
    char* out;
    uint32_t n, max;
} text;

static void add(text* t, const char* s) {
    for (; *s; s++) {
        if (t->n + 1u < t->max) {
            t->out[t->n++] = *s;
        }
    }
    if (t->max) {
        t->out[t->n < t->max ? t->n : t->max - 1u] = 0;
    }
}

static void add_char(text* t, char c) {
    char s[2] = {c, 0};
    add(t, s);
}

static void add_milli(text* t, int64_t v) {
    char d[24];
    uint32_t n = 0;
    if (v < 0) {
        add_char(t, '-');
        v = -v;
    }
    int64_t whole = v / MILLI, frac = v % MILLI;
    do {
        d[n++] = (char)('0' + whole % 10);
        whole /= 10;
    } while (whole);
    while (n) {
        add_char(t, d[--n]);
    }
    if (frac) {
        add_char(t, '.');
        for (int64_t p = MILLI / 10; frac; p /= 10) {
            add_char(t, (char)('0' + frac / p));
            frac %= p;
        }
    }
}

static void add_names(text* t, const char* names, const char* last) {
    for (const char* s = names; *s; s += length(s) + 1u) {
        if (s != names) {
            add(t, s[length(s) + 1u] ? ", " : last);
        }
        add(t, s);
    }
}

static int64_t scaled_max_milli(const scale* k) {
    return (int64_t)k->max * k->den * MILLI / k->num;
}

static int64_t top_milli(const field* f) {
    return f->kind == K_SCALED ? scaled_max_milli(f->more)
           : f->kind == K_HZ   ? (int64_t)cutoff_max_hz() * MILLI
                               : rate_milli(LAB_LFO_RATES - 1u);
}

static void allowed(text* t, const field* f) {
    switch (f->kind) {
    case K_INT:
    case K_VOICES:
        if (f->kind == K_VOICES) {
            add(t, "a list of chip voices ");
        }
        add_milli(t, (int64_t)f->lo * MILLI);
        add(t, " to ");
        add_milli(t, (int64_t)f->hi * MILLI);
        if (f->kind == K_VOICES) {
            add(t, ", like [1, 2, 3]");
        }
        return;
    case K_SCALED:
        add(t, "0 to ");
        add_milli(t, scaled_max_milli(f->more));
        return;
    case K_HZ:
        add(t, "0 to ");
        add_milli(t, (int64_t)cutoff_max_hz() * MILLI);
        return;
    case K_RATE:
        add_milli(t, rate_milli(0));
        add(t, " to ");
        add_milli(t, rate_milli(LAB_LFO_RATES - 1u));
        return;
    case K_BOOL: add(t, "true or false"); return;
    case K_ENUM: add_names(t, f->more, " or "); return;
    case K_SET:
        add_names(t, nth(f->more, 1), " and ");
        add(t, ", one or several joined with + (like \"");
        add(t, nth(f->more, 1));
        add(t, "+");
        add(t, nth(f->more, 2));
        add(t, "\"), or ");
        add(t, f->more);
        return;
    case K_NOTE:
        add(t, "-32 to 95 semitones from the key played, a fixed note \"C#0\" to \"B7\", or \"hold\"");
        return;
    case K_NAME: add(t, "up to 12 characters: A to Z, 0 to 9, space and !\"#$%&'()*+,-./:;<=>?@[\\]^_"); return;
    case K_FORMAT: add(t, "\"" PATCH_JSON_FORMAT "\""); return;
    case K_LIST:
        add(t, "a list of up to ");
        add_milli(t, (int64_t)((const list*)f->more)->max * MILLI);
        return;
    default: add(t, "an object { }"); return;
    }
}

typedef struct {
    patch_json* p;
    const char* s;
    uint32_t i, n, lfos, depth, format;
    uint8_t touched[LAB_SOUND_BYTES];
    char path[96];
    uint32_t path_n;
    text err;
} reader;

static int fail(reader* r, const char* what, const field* f) {
    if (r->err.n) {
        return -1;
    }
    add(&r->err, r->path_n ? r->path : "the patch");
    add(&r->err, r->path_n ? ": " : " ");
    add(&r->err, what);
    if (f) {
        add(&r->err, ". Allowed: ");
        allowed(&r->err, f);
    }
    add(&r->err, ".");
    return -1;
}

static int syntax(reader* r, const char* expected) {
    uint32_t line = 1, col = 1;
    for (uint32_t k = 0; k < r->i && k < r->n; k++) {
        col = r->s[k] == '\n' ? 1u : col + 1u;
        line += r->s[k] == '\n';
    }
    r->err.n = 0;
    add(&r->err, "the text is not valid JSON at line ");
    add_milli(&r->err, (int64_t)line * MILLI);
    add(&r->err, ", column ");
    add_milli(&r->err, (int64_t)col * MILLI);
    add(&r->err, ": expected ");
    add(&r->err, expected);
    add(&r->err, ".");
    return -1;
}

static char peek(reader* r) {
    while (r->i < r->n && (r->s[r->i] == ' ' || r->s[r->i] == '\t' || r->s[r->i] == '\n' ||
                           r->s[r->i] == '\r')) {
        r->i++;
    }
    return r->i < r->n ? r->s[r->i] : 0;
}

static int eat(reader* r, char c, const char* expected) {
    if (peek(r) != c) {
        return syntax(r, expected);
    }
    r->i++;
    return 0;
}

static int word(reader* r, const char* w) {
    uint32_t n = length(w);
    if (r->n - r->i < n || memcmp(r->s + r->i, w, n)) {
        return 0;
    }
    r->i += n;
    return 1;
}

static int string(reader* r, char* out, uint32_t max) {
    if (eat(r, '"', "a \"")) {
        return -1;
    }
    uint32_t n = 0;
    while (r->i < r->n && r->s[r->i] != '"') {
        char c = r->s[r->i++];
        if (c == '\\' && r->i < r->n) {
            c = r->s[r->i++];
            if (c == 'u') {
                uint32_t code = 0;
                for (uint32_t k = 0; k < 4u && r->i < r->n; k++) {
                    char h = r->s[r->i++];
                    code = code << 4 | (uint32_t)(h <= '9' ? h - '0' : (h | 0x20) - 'a' + 10);
                }
                c = code < 0x80u ? (char)code : '\x7f';
            } else if (c == 'n' || c == 't' || c == 'r' || c == 'b' || c == 'f') {
                c = '\x01';
            }
        }
        if (n + 1u < max) {
            out[n] = c;
        }
        n++;
    }
    if (eat(r, '"', "the end of the text, a \"")) {
        return -1;
    }
    out[n < max ? n : max - 1u] = 0;
    return (int)n;
}

static int number(reader* r, int64_t* milli, int* whole) {
    int neg = r->s[r->i] == '-';
    r->i += (uint32_t)neg;
    int64_t v = 0, frac = 0, p = MILLI;
    uint32_t digits = 0;
    while (r->i < r->n && r->s[r->i] >= '0' && r->s[r->i] <= '9') {
        v = v < (int64_t)1e12 ? v * 10 + (r->s[r->i] - '0') : v;
        r->i++;
        digits++;
    }
    *whole = 1;
    if (r->i < r->n && r->s[r->i] == '.') {
        r->i++;
        while (r->i < r->n && r->s[r->i] >= '0' && r->s[r->i] <= '9') {
            p /= 10;
            frac += p * (r->s[r->i] - '0');
            *whole &= r->s[r->i] == '0';
            r->i++;
            digits++;
        }
    }
    if (!digits) {
        return syntax(r, "a number");
    }
    if (r->i < r->n && (r->s[r->i] == 'e' || r->s[r->i] == 'E')) {
        return fail(r, "write the number plainly, without an exponent", NULL);
    }
    *milli = (neg ? -1 : 1) * (v * MILLI + frac);
    return 0;
}

static int object(reader* r, const field* table, uint32_t a, uint32_t b);

static const field* find(const field* t, const char* key) {
    for (; t->name; t++) {
        if (t->kind == K_ALSO) {
            const field* f = find(t->more, key);
            if (f) {
                return f;
            }
        } else if (!strcmp(t->name, key)) {
            return t;
        }
    }
    return NULL;
}

static void add_fields(text* t, const field* f, int* first) {
    for (; f->name; f++) {
        if (f->kind == K_ALSO) {
            add_fields(t, f->more, first);
            continue;
        }
        if (!*first) {
            add(t, ", ");
        }
        *first = 0;
        add(t, f->name);
    }
}

static int unknown(reader* r, const field* t) {
    char w[PATCH_JSON_ERROR_MAX];
    text m = {w, 0, sizeof w};
    int first = 1;
    add(&m, "no such field. Allowed here: ");
    add_fields(&m, t, &first);
    return fail(r, w, NULL);
}

static void push(reader* r, const char* name, int index) {
    text t = {r->path, r->path_n, sizeof r->path};
    if (index >= 0) {
        add_char(&t, '[');
        add_milli(&t, (int64_t)index * MILLI);
        add_char(&t, ']');
    } else {
        if (r->path_n) {
            add_char(&t, '.');
        }
        add(&t, name);
    }
    r->path_n = t.n;
}

static int is_number(char c) {
    return c == '-' || (c >= '0' && c <= '9');
}

static int need(reader* r, const field* f, char c, int ok, const char* what) {
    if (!ok) {
        if (!c || (c != '"' && c != '{' && c != '[' && c != 't' && c != 'f' && !is_number(c))) {
            return syntax(r, "a value");
        }
        return fail(r, what, f);
    }
    return 0;
}

static int range(reader* r, const field* f, int64_t milli, int64_t lo, int64_t hi) {
    char w[48];
    text t = {w, 0, sizeof w};
    add_milli(&t, milli);
    add(&t, milli < lo ? " is too low" : " is too high");
    return milli < lo || milli > hi ? fail(r, w, f) : 0;
}

static int names_index(const char* names, const char* s) {
    int i = 0;
    for (const char* n = names; *n; n += length(n) + 1u, i++) {
        if (!strcmp(n, s)) {
            return i;
        }
    }
    return -1;
}

static int not_allowed(reader* r, const field* f, const char* got) {
    char w[64];
    text t = {w, 0, sizeof w};
    add(&t, "\"");
    add(&t, got);
    add(&t, "\" is not allowed");
    return fail(r, w, f);
}

static int set_bits(const char* names, const char* text_in) {
    char s[64];
    int bits = 0;
    memcpy(s, text_in, sizeof s);
    if (!strcmp(s, names)) {
        return 0;
    }
    for (char* part = s; *part;) {
        char* end = part;
        while (*end && *end != '+') {
            end++;
        }
        char keep = *end;
        *end = 0;
        while (*part == ' ') {
            part++;
        }
        for (char* e = end; e > part && e[-1] == ' ';) {
            *--e = 0;
        }
        int i = names_index(names, part);
        if (i < 1 || bits >> (i - 1) & 1) {
            return -1;
        }
        bits |= 1 << (i - 1);
        if (!keep) {
            break;
        }
        part = end + 1;
    }
    return bits ? bits : -1;
}

static int note_of(const char* s) {
    if (!strcmp(s, "hold")) {
        return NOTE_HOLD;
    }
    char name[3] = {s[0], s[1] == '#' ? '#' : 0, 0};
    int i = names_index(NOTE_NAMES, name);
    const char* o = s + 1 + (s[1] == '#' || s[1] == '-');
    if (i < 0 || o[0] < '0' || o[0] > '9' || o[1]) {
        return -1;
    }
    int n = (o[0] - '0') * 12 + i;
    return n >= 1 && n <= (int)LAB_TABLE_NOTE_UP_MAX ? (int)NOTE_HOLD + n : -1;
}

static int list_of(reader* r, const field* f, uint32_t a, uint32_t b);

static int value(reader* r, const field* f, uint32_t a, uint32_t b) {
    uint8_t* s = r->p->sound;
    char c = peek(r);
    char str[64];
    int64_t milli = 0;
    int whole = 1;
    if (c == '"' && (f->kind == K_ENUM || f->kind == K_SET || f->kind == K_NOTE || f->kind == K_NAME ||
                     f->kind == K_FORMAT)) {
        if (string(r, str, sizeof str) < 0) {
            return -1;
        }
    } else if (is_number(c) && (f->kind <= K_SCALED || f->kind == K_HZ || f->kind == K_RATE ||
                                f->kind == K_NOTE)) {
        if (number(r, &milli, &whole)) {
            return -1;
        }
    }
    switch (f->kind) {
    case K_INT:
    case K_NOTE: {
        if (f->kind == K_NOTE && c == '"') {
            int n = note_of(str);
            if (n < 0) {
                return not_allowed(r, f, str);
            }
            put(s, r->touched, f, a, b, (uint32_t)n);
            return 0;
        }
        if (need(r, f, c, is_number(c), "must be a whole number") ||
            need(r, f, c, whole, "must be a whole number") ||
            range(r, f, milli, (int64_t)f->lo * MILLI, (int64_t)f->hi * MILLI)) {
            return -1;
        }
        int32_t v = (int32_t)(milli / MILLI) - (f->flags & F_PLUS1 ? 1 : 0);
        put(s, r->touched, f, a, b, (uint32_t)(f->kind == K_NOTE && v < 0 ? (int32_t)NOTE_HOLD + v : v));
        return 0;
    }
    case K_SCALED:
    case K_HZ:
    case K_RATE: {
        if (need(r, f, c, is_number(c), "must be a number")) {
            return -1;
        }
        const scale* k = f->more;
        if (range(r, f, milli, f->kind == K_RATE ? rate_milli(0) : 0, top_milli(f))) {
            return -1;
        }
        int64_t reg = f->kind == K_SCALED ? scaled_reg(k, milli)
                      : f->kind == K_HZ   ? cutoff_of(milli)
                                          : rate_of(milli);
        put(s, r->touched, f, a, b, (uint32_t)reg);
        return 0;
    }
    case K_BOOL: {
        int yes = word(r, "true");
        if (yes || word(r, "false")) {
            put(s, r->touched, f, a, b, (uint32_t)yes);
            return 0;
        }
        return need(r, f, c, 0, "must be true or false");
    }
    case K_ENUM:
    case K_SET: {
        if (need(r, f, c, c == '"', "must be text")) {
            return -1;
        }
        int v = f->kind == K_ENUM ? names_index(f->more, str) : set_bits(f->more, str);
        if (v < 0) {
            return not_allowed(r, f, str);
        }
        put(s, r->touched, f, a, b, (uint32_t)v);
        return 0;
    }
    case K_NAME: {
        if (need(r, f, c, c == '"', "must be text")) {
            return -1;
        }
        uint32_t n = length(str);
        if (n > LAB_SOUND_NAME) {
            return fail(r, "is too long", f);
        }
        for (uint32_t k = 0; k < n; k++) {
            char ch = str[k] >= 'a' && str[k] <= 'z' ? (char)(str[k] - 32) : str[k];
            if (ch < NAME_FIRST || ch > NAME_LAST) {
                return fail(r, "has a character the FM-1 cannot show", f);
            }
            r->p->name[k] = ch;
        }
        r->p->name[n] = 0;
        return 0;
    }
    case K_FORMAT:
        if (need(r, f, c, c == '"', "must be text")) {
            return -1;
        }
        if (strcmp(str, PATCH_JSON_FORMAT)) {
            return not_allowed(r, f, str);
        }
        r->format = 1;
        return 0;
    case K_VOICES: {
        uint32_t bits = 0;
        if (need(r, f, c, c == '[', "must be a list")) {
            return -1;
        }
        r->i++;
        if (peek(r) == ']') {
            r->i++;
        } else {
            for (;;) {
                c = peek(r);
                if (!is_number(c)) {
                    return need(r, f, c, 0, "must list whole numbers");
                }
                if (number(r, &milli, &whole) || need(r, f, c, whole, "must list whole numbers") ||
                    range(r, f, milli, (int64_t)f->lo * MILLI, (int64_t)f->hi * MILLI)) {
                    return -1;
                }
                bits |= 1u << (milli / MILLI - 1);
                if (peek(r) == ']') {
                    r->i++;
                    break;
                }
                if (eat(r, ',', "a , or ]")) {
                    return -1;
                }
            }
        }
        put(s, r->touched, f, a, b, bits);
        return 0;
    }
    case K_GROUP:
        if (need(r, f, c, c == '{', "must be an object")) {
            return -1;
        }
        return object(r, f->more, a + f->at, b);
    default:
        if (need(r, f, c, c == '[', "must be a list")) {
            return -1;
        }
        return list_of(r, f, a, b);
    }
}

static int list_of(reader* r, const field* f, uint32_t a, uint32_t b) {
    (void)a;
    (void)b;
    const list* l = f->more;
    uint32_t n = 0;
    r->i++;
    if (peek(r) == ']') {
        r->i++;
    } else {
        for (;;) {
            if (n == l->max) {
                return fail(r, "has too many", f);
            }
            uint32_t keep = r->path_n;
            push(r, NULL, (int)n);
            char c = peek(r);
            if (need(r, f, c, c == '{', "must list objects { }") ||
                object(r, l->sub, l->first + n * l->stride, l->tuning + n * l->tuning_stride)) {
                return -1;
            }
            r->path_n = keep;
            r->path[keep] = 0;
            n++;
            if (peek(r) == ']') {
                r->i++;
                break;
            }
            if (eat(r, ',', "a , or ]")) {
                return -1;
            }
        }
    }
    if (l->kind == L_LAYERS) {
        mark(r->p->sound, r->touched, LAB_SOUND_LAYERS, 0xFFu, n);
    } else if (l->kind == L_STEPS) {
        mark(r->p->sound, r->touched, LAB_SOUND_TABLE, LAB_TABLE_COUNT_MASK, n);
    } else {
        r->lfos = n;
    }
    return 0;
}

static int object(reader* r, const field* table, uint32_t a, uint32_t b) {
    if (++r->depth > PATCH_JSON_DEPTH_MAX) {
        return fail(r, "is nested too deep", NULL);
    }
    if (eat(r, '{', "a {")) {
        return -1;
    }
    if (peek(r) == '}') {
        r->i++;
        r->depth--;
        return 0;
    }
    for (;;) {
        char key[32];
        if (string(r, key, sizeof key) < 0 || eat(r, ':', "a :")) {
            return -1;
        }
        uint32_t keep = r->path_n;
        push(r, key, -1);
        const field* f = find(table, key);
        if (!f) {
            return unknown(r, table);
        }
        if (peek(r) == 'n' && word(r, "null")) {
        } else if (value(r, f, a, b)) {
            return -1;
        }
        r->path_n = keep;
        r->path[keep] = 0;
        if (peek(r) == '}') {
            r->i++;
            r->depth--;
            return 0;
        }
        if (eat(r, ',', "a , or }")) {
            return -1;
        }
    }
}

static void fill(uint8_t* s, const uint8_t* touched, const uint8_t* from, uint32_t lfos) {
    uint32_t layers = from[LAB_SOUND_LAYERS] <= LIVE_LAYERS_MAX ? from[LAB_SOUND_LAYERS] : 0u;
    for (uint32_t n = 0; n < layers; n++) {
        for (uint32_t k = 0; k < VOICE_BYTES; k++) {
            uint32_t at = LAB_SOUND_LAYER_VOICE + n * VOICE_BYTES + k;
            s[at] = (uint8_t)((s[at] & touched[at]) | (from[LAB_SOUND_CONTROL + k] & ~touched[at]));
        }
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint32_t alg = from[LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS];
        for (uint32_t c = LIVE_SLOT_AMOUNT; alg && alg < 8u && c < LIVE_SLOT_FIELDS; c++) {
            uint32_t at = LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS + c;
            s[at] = (uint8_t)((s[at] & touched[at]) |
                              (live_fx_defaults[k][alg][c - LIVE_SLOT_AMOUNT] & ~touched[at]));
        }
    }
    uint32_t head = from[LAB_SOUND_TABLE], steps = head & LAB_TABLE_COUNT_MASK;
    uint32_t layer = (head & LAB_TABLE_LAYER_MASK) >> LAB_TABLE_LAYER_SHIFT;
    if (!(touched[LAB_SOUND_TABLE] & LAB_TABLE_ON)) {
        s[LAB_SOUND_TABLE] = (uint8_t)((s[LAB_SOUND_TABLE] & ~LAB_TABLE_ON) | (steps ? LAB_TABLE_ON : 0u));
    }
    uint8_t control = from[layer && layer <= LIVE_LAYERS_MAX
                               ? LAB_SOUND_LAYER_VOICE + (layer - 1u) * VOICE_BYTES
                               : LAB_SOUND_CONTROL];
    for (uint32_t n = 0; n < steps && n < LAB_TABLE_STEPS_MAX; n++) {
        uint32_t at = LAB_SOUND_TABLE_STEPS + 2u * n;
        s[at] = (uint8_t)((s[at] & touched[at]) | (control & CONTROL_KEPT & ~touched[at]));
    }
    for (uint32_t n = 0; n < lfos; n++) {
        uint32_t at = LAB_SOUND_LFO + n * LAB_SOUND_LFO_BYTES;
        if (!(touched[at] & LAB_LFO_SHAPE_MASK)) {
            s[at] = (uint8_t)((s[at] & ~LAB_LFO_SHAPE_MASK) | LAB_LFO_TRIANGLE);
        }
    }
}

static int whole_fail(reader* r, const char* path, const char* what) {
    r->path_n = 0;
    r->path[0] = 0;
    if (path) {
        push(r, path, -1);
    }
    return fail(r, what, NULL);
}

static int checked(reader* r) {
    const uint8_t* s = r->p->sound;
    uint32_t steps = s[LAB_SOUND_TABLE] & LAB_TABLE_COUNT_MASK;
    if (!r->format) {
        return whole_fail(r, "format", "is missing. Allowed: \"" PATCH_JSON_FORMAT "\"");
    }
    if (s[LAB_SOUND_LAYERS] && s[LAB_SOUND_MONO_SYNC]) {
        return whole_fail(r, "layers", "cannot be used with sync_lead on: remove one of them");
    }
    if ((s[LAB_SOUND_TABLE] & LAB_TABLE_ON) && !steps) {
        return whole_fail(r, "table.steps", "an on table needs at least one step");
    }
    if (s[LAB_SOUND_TABLE_LOOP] > steps) {
        return whole_fail(r, "table.loop", "is past the last step: 0 plays once, 1 to the number of steps loops back to that step");
    }
    if (!lab_sound_valid(s)) {
        return whole_fail(r, NULL, "is not a sound LAB can hold");
    }

    uint32_t heard = lab_audible(s, SID_MODEL_6581) | lab_audible(s, SID_MODEL_8580) << 1;
    if (heard == 3u) {
        return PATCH_JSON_OK;
    }
    char w[PATCH_JSON_ERROR_MAX];
    text m = {w, 0, sizeof w};
    add(&m, heard ? "is silent on the " : "is silent on both chips");
    add(&m, heard == 1u ? "8580" : heard == 2u ? "6581" : "");
    add(&m, ": nothing would be heard. Check the wave (none plays nothing; on a 6581 two waves "
            "together are very quiet), the envelope (a sustain of 0 needs a decay), the filter (a "
            "cutoff far from the notes), the pulse width (0 or 100 is silent) and a volume LFO");
    whole_fail(r, NULL, w);
    return heard ? PATCH_JSON_QUIET : -1;
}

int patch_json_read(patch_json* p, const char* s, uint32_t len) {
    reader r;
    memset(&r, 0, sizeof r);
    r.p = p;
    r.s = s;
    r.n = len;
    r.err = (text){p->error, 0, sizeof p->error};
    p->error[0] = 0;
    memcpy(p->sound, DEFAULTS, LAB_SOUND_BYTES);
    memcpy(p->name, PATCH_JSON_NAME, sizeof PATCH_JSON_NAME);
    while (r.i < r.n && s[r.i] != '{') {
        r.i++;
    }
    if (r.i == r.n) {
        r.i = 0;
        return syntax(&r, "a { to start the patch");
    }
    if (object(&r, ROOT, 0, LAB_SOUND_LAYER_TUNING)) {
        return -1;
    }
    fill(p->sound, r.touched, p->sound, r.lfos);
    return checked(&r);
}

typedef struct {
    text t;
    const uint8_t *s, *ref;
    const char* name;
} writer;

static uint32_t list_count(const list* l, const uint8_t* s) {
    if (l->kind == L_LAYERS) {
        return s[LAB_SOUND_LAYERS] <= LIVE_LAYERS_MAX ? s[LAB_SOUND_LAYERS] : 0u;
    }
    if (l->kind == L_STEPS) {
        uint32_t n = s[LAB_SOUND_TABLE] & LAB_TABLE_COUNT_MASK;
        return n <= LAB_TABLE_STEPS_MAX ? n : 0u;
    }
    uint32_t n = 0;
    for (uint32_t k = 0; k < LAB_SOUND_LFOS; k++) {
        const uint8_t* f = &s[LAB_SOUND_LFO + k * LAB_SOUND_LFO_BYTES];
        n = f[0] | f[1] | f[2] ? k + 1u : n;
    }
    return n;
}

static int shown(const writer* w, const field* f, uint32_t a, uint32_t b, int force);

enum { SHOW_CHANGED, SHOW_MARKED, SHOW_ALL };

static int inner(const field* f, int force) {
    return force == SHOW_ALL || (force == SHOW_MARKED && (f->flags & F_ALWAYS)) ? SHOW_ALL
                                                                                : SHOW_CHANGED;
}

static int any_shown(const writer* w, const field* t, uint32_t a, uint32_t b, int force) {
    for (; t->name; t++) {
        if (shown(w, t, a, b, force)) {
            return 1;
        }
    }
    return 0;
}

static int shown(const writer* w, const field* f, uint32_t a, uint32_t b, int force) {
    switch (f->kind) {
    case K_FORMAT:
    case K_NAME: return 1;
    case K_ALSO: return any_shown(w, f->more, a, b, force);
    case K_GROUP: return any_shown(w, f->more, a + f->at, b, inner(f, force));
    case K_LIST: return list_count(f->more, w->s) != 0u;
    default: return inner(f, force) || get(w->s, f, a, b) != get(w->ref, f, a, b);
    }
}

static double hz_of(const field* f, uint32_t reg) {
    if (f->kind == K_HZ) {
        return (double)cutoff_hz(reg);
    }
    if (f->kind == K_RATE) {
        return (double)rate_milli(reg) / MILLI;
    }
    const scale* k = f->more;
    return (double)reg * k->den / k->num;
}

static int64_t reg_of(const field* f, int64_t milli) {
    if (f->kind == K_HZ) {
        return cutoff_of(milli);
    }
    if (f->kind == K_RATE) {
        return rate_of(milli);
    }
    return scaled_reg(f->more, milli);
}

static int64_t shortest(const field* f, uint32_t reg) {
    double v = hz_of(f, reg) * MILLI;
    for (int64_t unit = MILLI; unit >= 1; unit /= 10) {
        int64_t c = (int64_t)(v / (double)unit + 0.5) * unit;
        if (c <= top_milli(f) && reg_of(f, c) == (int64_t)reg) {
            return c;
        }
    }
    return (int64_t)(v + 0.5);
}

static void quoted(text* t, const char* s) {
    add_char(t, '"');
    for (; *s; s++) {
        if (*s == '"' || *s == '\\') {
            add_char(t, '\\');
        }
        add_char(t, *s);
    }
    add_char(t, '"');
}

static void write_set(text* t, const char* names, uint32_t bits) {
    if (!bits) {
        quoted(t, names);
        return;
    }
    char buf[48];
    text b = {buf, 0, sizeof buf};
    for (uint32_t i = 0; bits >> i; i++) {
        if (bits >> i & 1u) {
            if (b.n) {
                add_char(&b, '+');
            }
            add(&b, nth(names, i + 1u));
        }
    }
    quoted(t, buf);
}

static void write_note(text* t, uint32_t note) {
    if (note == NOTE_HOLD) {
        quoted(t, "hold");
    } else if (lab_table_absolute((uint8_t)note)) {
        char buf[5];
        text b = {buf, 0, sizeof buf};
        add(&b, nth(NOTE_NAMES, (note - NOTE_HOLD) % 12u));
        add_char(&b, (char)('0' + (note - NOTE_HOLD) / 12u));
        quoted(t, buf);
    } else {
        add_milli(t, (int64_t)lab_table_offset((uint8_t)note) * MILLI);
    }
}

static void write_object(writer* w, const field* t, uint32_t a, uint32_t b, int force, int indent);

static int has_list(const field* t) {
    for (; t->name; t++) {
        if (t->kind == K_LIST) {
            return 1;
        }
    }
    return 0;
}

static void newline(text* t, int indent) {
    if (indent >= 0) {
        add_char(t, '\n');
    }
    for (int i = 0; i < indent; i++) {
        add_char(t, ' ');
    }
}

static void write_value(writer* w, const field* f, uint32_t a, uint32_t b, int force, int indent) {
    text* t = &w->t;
    uint32_t v = get(w->s, f, a, b);
    switch (f->kind) {
    case K_FORMAT: quoted(t, PATCH_JSON_FORMAT); return;
    case K_NAME: quoted(t, w->name); return;
    case K_INT: {
        int32_t n = f->lo < 0 ? (int8_t)v : (int32_t)v;
        add_milli(t, (int64_t)(n + (f->flags & F_PLUS1 ? 1 : 0)) * MILLI);
        return;
    }
    case K_SCALED:
    case K_HZ:
    case K_RATE: add_milli(t, shortest(f, v)); return;
    case K_BOOL: add(t, v ? "true" : "false"); return;
    case K_ENUM: quoted(t, nth(f->more, v)); return;
    case K_SET: write_set(t, f->more, v); return;
    case K_NOTE: write_note(t, v); return;
    case K_VOICES: {
        add_char(t, '[');
        for (uint32_t k = 0, first = 1; k < LIVE_VOICES; k++) {
            if (v >> k & 1u) {
                add(t, first ? "" : ", ");
                add_char(t, (char)('1' + k));
                first = 0;
            }
        }
        add_char(t, ']');
        return;
    }
    case K_GROUP:
        write_object(w, f->more, a + f->at, b, inner(f, force),
                     indent >= 0 && has_list(f->more) ? indent + 2 : -1);
        return;
    default: {
        const list* l = f->more;
        uint32_t n = list_count(l, w->s);
        add_char(t, '[');
        for (uint32_t k = 0; k < n; k++) {
            add(t, k ? "," : "");
            newline(t, indent + 2);
            write_object(w, l->sub, l->first + k * l->stride, l->tuning + k * l->tuning_stride, 0, -1);
        }
        newline(t, indent);
        add_char(t, ']');
        return;
    }
    }
}

static void write_fields(writer* w, const field* f, uint32_t a, uint32_t b, int force, int indent,
                         int* first) {
    for (; f->name; f++) {
        if (f->kind == K_ALSO) {
            write_fields(w, f->more, a, b, force, indent, first);
            continue;
        }
        if (!shown(w, f, a, b, force)) {
            continue;
        }
        add(&w->t, *first ? "" : indent < 0 ? ", " : ",");
        newline(&w->t, indent);
        *first = 0;
        quoted(&w->t, f->name);
        add(&w->t, ": ");
        write_value(w, f, a, b, force, indent);
    }
}

static void write_object(writer* w, const field* t, uint32_t a, uint32_t b, int force, int indent) {
    int first = 1;
    add_char(&w->t, '{');
    write_fields(w, t, a, b, force, indent, &first);
    newline(&w->t, indent - 2);
    add_char(&w->t, '}');
}

uint32_t patch_json_write(const uint8_t sound[LAB_SOUND_BYTES], const char* name, char* out,
                          uint32_t max) {
    uint8_t ref[LAB_SOUND_BYTES], none[LAB_SOUND_BYTES] = {0};
    memcpy(ref, DEFAULTS, LAB_SOUND_BYTES);
    ref[LAB_SOUND_LAYERS] = sound[LAB_SOUND_LAYERS];
    fill(ref, none, sound, list_count(&LFOS, sound));
    char shown_name[LAB_SOUND_NAME + 1u];
    uint32_t n = 0;
    for (; name && name[n] && n < LAB_SOUND_NAME; n++) {
        char c = name[n];
        shown_name[n] = c >= 'a' && c <= 'z' ? (char)(c - 32) : c < NAME_FIRST || c > NAME_LAST ? ' ' : c;
    }
    shown_name[n] = 0;
    writer w = {{out, 0, max}, sound, ref, n ? shown_name : PATCH_JSON_NAME};
    write_object(&w, ROOT, 0, LAB_SOUND_LAYER_TUNING, SHOW_MARKED, 2);
    add_char(&w.t, '\n');
    return w.t.n + 1u < max ? w.t.n : 0u;
}

static const char* json_type(const field* f) {
    static const char* const KINDS[] = {"integer", "number", "boolean", "enum",   "set",
                                        "voices",  "number", "number",  "note",   "name",
                                        "format",  "object", "object",  "list"};
    return KINDS[f->kind];
}

static void describe(text* t, const field* f, char* path, uint32_t n, uint32_t max) {
    for (; f->name; f++) {
        if (f->kind == K_ALSO) {
            describe(t, f->more, path, n, max);
            continue;
        }
        text p = {path, n, max};
        if (n) {
            add_char(&p, '.');
        }
        add(&p, f->name);
        add(t, path);
        add_char(t, '\t');
        add(t, json_type(f));
        add_char(t, '\t');
        if (f->kind == K_ENUM || f->kind == K_SET) {
            add_names(t, f->more, ",");
        }
        add_char(t, '\t');
        allowed(t, f);
        add_char(t, '\n');
        if (f->kind == K_GROUP) {
            describe(t, f->more, path, p.n, max);
        } else if (f->kind == K_LIST) {
            add(&p, "[]");
            describe(t, ((const list*)f->more)->sub, path, p.n, max);
        }
        path[n] = 0;
    }
}

uint32_t patch_json_fields(char* out, uint32_t max) {
    char path[96] = {0};
    text t = {out, 0, max};
    describe(&t, ROOT, path, 0, sizeof path);
    return t.n + 1u < max ? t.n : 0u;
}

int patch_json_song(gt2_song* s, const uint8_t sound[LAB_SOUND_BYTES], const char* name,
                    uint32_t model) {
    static const uint8_t first[GT2_TABLES] = {1u, 1u, 1u, 1u};
    lab_song_instr in;
    if (gt2_song_clear(s)) {
        return -1;
    }
    lab_song_encode(sound, name, 0, model, first, &in);
    memcpy(s->instr[0], in.instr, GT2_INSTR_BYTES);
    for (uint32_t t = 0; t < GT2_TABLES; t++) {
        memcpy(s->ltable[t], in.left[t], in.rows[t]);
        memcpy(s->rtable[t], in.right[t], in.rows[t]);
    }
    memset(s->name, 0, sizeof s->name);
    for (uint32_t k = 0; k < LAB_SOUND_NAME && name[k]; k++) {
        s->name[k] = name[k];
    }
    gt2_saved_recount(s);
    return lab_song_keep(s, 1u, &in, sound);
}

uint32_t patch_json_song_sound(const gt2_song* s, uint32_t instr, char* out, uint32_t max) {
    uint8_t rows[GT2_TABLES], sound[LAB_SOUND_BYTES];
    char name[LAB_SOUND_NAME + 1u];
    if (instr < 1u || instr > GT2_MAX_INSTR || !lab_song_kept(s, instr, rows, sound)) {
        return 0;
    }
    lab_song_name(s, instr, name);
    return patch_json_write(sound, name, out, max);
}
