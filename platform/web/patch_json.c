// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "patch_json.h"

#include <string.h>

#include "lab_song.h"
#include "live.h"
#include "player/song_ctl.h"
#include "song/gt2_sng.h"
#include "song/song_ext.h"
#include "song_io.h"
#include "song_tempo.h"
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
    K_LIST,
    K_PITCH
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
#define ESCAPED_CONTROL '\x01'

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
    case K_PITCH: add(t, "a note \"C0\" to \"G#7\" (\"C3\" is the middle of STEP's range)"); return;
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
    uint32_t path_n, base_n;
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
                c = ESCAPED_CONTROL;
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

static int pitch_of(const char* s) {
    char name[3] = {s[0], s[1] == '#' ? '#' : 0, 0};
    int i = names_index(NOTE_NAMES, name);
    const char* o = s + 1 + (s[1] == '#' || s[1] == '-');
    if (i < 0 || o[0] < '0' || o[0] > '9' || o[1]) {
        return -1;
    }
    int n = (o[0] - '0') * 12 + i;
    return n <= (int)GT2_NOTE_MAX ? n : -1;
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
                     f->kind == K_FORMAT || f->kind == K_PITCH)) {
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
    case K_PITCH: {
        if (need(r, f, c, c == '"', "must be text")) {
            return -1;
        }
        int n = pitch_of(str);
        if (n < 0) {
            return not_allowed(r, f, str);
        }
        put(s, r->touched, f, a, b, (uint32_t)n);
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
    r->path_n = r->base_n;
    r->path[r->base_n] = 0;
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
    uint32_t voices = s[LAB_SOUND_FILTER_VOICES] & 0x07u;
    if (!s[LAB_SOUND_LAYERS] && voices && voices != 0x07u) {
        return whole_fail(r, "filter.voices",
                          "a one-oscillator sound plays each key on the next free voice, so the filter "
                          "takes all three voices or none. Allowed: [1, 2, 3] or []");
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

    return PATCH_JSON_OK;
}

static int skip_value(reader* r) {
    char c = peek(r), junk[2];
    int64_t milli;
    int whole;
    if (c == '"') {
        return string(r, junk, sizeof junk) < 0 ? -1 : 0;
    }
    if (is_number(c)) {
        return number(r, &milli, &whole);
    }
    if (word(r, "true") || word(r, "false") || word(r, "null")) {
        return 0;
    }
    if (c != '{' && c != '[') {
        return syntax(r, "a value");
    }
    char end = c == '{' ? '}' : ']';
    r->i++;
    if (peek(r) == end) {
        r->i++;
        return 0;
    }
    for (;;) {
        if (c == '{' && (string(r, junk, sizeof junk) < 0 || eat(r, ':', "a :"))) {
            return -1;
        }
        if (skip_value(r)) {
            return -1;
        }
        if (peek(r) == end) {
            r->i++;
            return 0;
        }
        if (eat(r, ',', end == '}' ? "a , or }" : "a , or ]")) {
            return -1;
        }
    }
}

static void reader_init(reader* r, patch_json* p, const char* s, uint32_t start, uint32_t end,
                        char* error) {
    memset(r, 0, sizeof *r);
    r->p = p;
    r->s = s;
    r->i = start;
    r->n = end;
    r->err = (text){error, 0, PATCH_JSON_ERROR_MAX};
    error[0] = 0;
}

static int read_part(patch_json* p, const char* s, uint32_t start, uint32_t end, const char* prefix,
                     char* error) {
    reader r;
    reader_init(&r, p, s, start, end, error);
    memcpy(p->sound, DEFAULTS, LAB_SOUND_BYTES);
    memcpy(p->name, PATCH_JSON_NAME, sizeof PATCH_JSON_NAME);
    if (prefix) {
        push(&r, prefix, -1);
        r.base_n = r.path_n;
        r.format = 1;
    }
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

int patch_json_read(patch_json* p, const char* s, uint32_t len) {
    return read_part(p, s, 0, len, NULL, p->error);
}

static int top_format(reader* r, char* format, uint32_t max) {
    format[0] = 0;
    while (r->i < r->n && r->s[r->i] != '{') {
        r->i++;
    }
    if (eat(r, '{', "a { to start the bank")) {
        return -1;
    }
    if (peek(r) == '}') {
        return 0;
    }
    for (;;) {
        char key[32];
        if (string(r, key, sizeof key) < 0 || eat(r, ':', "a :")) {
            return -1;
        }
        if (!strcmp(key, "format") && peek(r) == '"') {
            if (string(r, format, max) < 0) {
                return -1;
            }
        } else if (skip_value(r)) {
            return -1;
        }
        if (peek(r) == '}') {
            return 0;
        }
        if (eat(r, ',', "a , or }")) {
            return -1;
        }
    }
}

int patch_json_is_bank(const char* s, uint32_t len) {
    char error[PATCH_JSON_ERROR_MAX];
    char format[24];
    reader r;
    reader_init(&r, NULL, s, 0, len, error);
    return !top_format(&r, format, sizeof format) && !strcmp(format, PATCH_BANK_FORMAT);
}

#define BANK_NAME                                                                                  \
    "1 to 16 characters: A to Z, 0 to 9, space and !\"#$%&'()*+,-./:;<=>?@[\\]^_, not starting with "  \
    "a space"
#define BANK_PATCHES "a list of 1 to 16 patches"

static int bank_fail(reader* r, const char* what, const char* allowed_text) {
    char w[PATCH_JSON_ERROR_MAX];
    text m = {w, 0, sizeof w};
    add(&m, what);
    add(&m, ". Allowed: ");
    add(&m, allowed_text);
    return fail(r, w, NULL);
}

static int bank_name(reader* r, patch_bank* b) {
    char name[64];
    int n = string(r, name, sizeof name);
    if (n < 0) {
        return -1;
    }
    if (n < 1 || n > (int)PATCH_BANK_NAME || name[0] == ' ') {
        return bank_fail(r, n > (int)PATCH_BANK_NAME ? "is too long" : "is not a name", BANK_NAME);
    }
    for (int k = 0; k < n; k++) {
        char c = name[k] >= 'a' && name[k] <= 'z' ? (char)(name[k] - 32) : name[k];
        if (c < NAME_FIRST || c > NAME_LAST) {
            return bank_fail(r, "has a character the FM-1 cannot show", BANK_NAME);
        }
        b->name[k] = c;
    }
    b->name[n] = 0;
    return 0;
}

static int bank_patches(reader* r, patch_bank* b, int* quiet) {
    if (eat(r, '[', "a [")) {
        return -1;
    }
    if (peek(r) == ']') {
        r->i++;
        return 0;
    }
    for (;;) {
        if (b->count == PATCH_BANK_MAX) {
            return bank_fail(r, "has too many", BANK_PATCHES);
        }
        char prefix[16];
        text t = {prefix, 0, sizeof prefix};
        add(&t, "patches[");
        add_milli(&t, (int64_t)b->count * MILLI);
        add(&t, "]");
        uint32_t start = r->i;
        if (peek(r) != '{') {
            return fail(r, "must list objects { }", NULL);
        }
        if (skip_value(r)) {
            return -1;
        }
        int got = read_part(&b->part, r->s, start, r->i, prefix, b->part.error);
        if (got < 0 || (got == PATCH_JSON_QUIET && !*quiet)) {
            memcpy(r->err.out, b->part.error, PATCH_JSON_ERROR_MAX);
            r->err.n = length(r->err.out);
        }
        if (got < 0) {
            return -1;
        }
        *quiet |= got == PATCH_JSON_QUIET;
        memcpy(b->sounds[b->count], b->part.sound, LAB_SOUND_BYTES);
        memcpy(b->names[b->count], b->part.name, sizeof b->names[0]);
        b->count++;
        if (peek(r) == ']') {
            r->i++;
            return 0;
        }
        if (eat(r, ',', "a , or ]")) {
            return -1;
        }
    }
}

int patch_json_read_bank(patch_bank* b, const char* s, uint32_t len) {
    reader r;
    int format = 0, quiet = 0;
    reader_init(&r, &b->part, s, 0, len, b->error);
    b->count = 0;
    memcpy(b->name, PATCH_BANK_NAME_DEFAULT, sizeof PATCH_BANK_NAME_DEFAULT);
    while (r.i < r.n && s[r.i] != '{') {
        r.i++;
    }
    if (eat(&r, '{', "a { to start the bank")) {
        return -1;
    }
    while (peek(&r) != '}') {
        char key[32];
        if (string(&r, key, sizeof key) < 0 || eat(&r, ':', "a :")) {
            return -1;
        }
        r.path_n = 0;
        push(&r, key, -1);
        if (!strcmp(key, "format")) {
            char f[24];
            if (string(&r, f, sizeof f) < 0) {
                return -1;
            }
            if (strcmp(f, PATCH_BANK_FORMAT)) {
                char w[48];
                text m = {w, 0, sizeof w};
                add(&m, "\"");
                add(&m, f);
                add(&m, "\" is not allowed");
                return bank_fail(&r, w, "\"" PATCH_BANK_FORMAT "\"");
            }
            format = 1;
        } else if (!strcmp(key, "name")) {
            if (bank_name(&r, b)) {
                return -1;
            }
        } else if (!strcmp(key, "patches")) {
            if (bank_patches(&r, b, &quiet)) {
                return -1;
            }
        } else {
            return fail(&r, "no such field. Allowed here: format, name, patches", NULL);
        }
        if (peek(&r) == ',') {
            r.i++;
        } else if (peek(&r) != '}') {
            return syntax(&r, "a , or }");
        }
    }
    if (!format) {
        return whole_fail(&r, "format", "is missing. Allowed: \"" PATCH_BANK_FORMAT "\"");
    }
    if (!b->count) {
        return whole_fail(&r, "patches", "is empty. Allowed: " BANK_PATCHES);
    }
    if (!quiet) {
        b->error[0] = 0;
    }
    return quiet ? PATCH_JSON_QUIET : PATCH_JSON_OK;
}

typedef struct {
    text t;
    const uint8_t *s, *ref;
    const char* name;
    int in_bank;
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
        if (!shown(w, f, a, b, force) || (f->kind == K_FORMAT && w->in_bank)) {
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

static void write_patch(text* t, const uint8_t sound[LAB_SOUND_BYTES], const char* name, int indent) {
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
    writer w = {*t, sound, ref, n ? shown_name : PATCH_JSON_NAME, indent > 2};
    write_object(&w, ROOT, 0, LAB_SOUND_LAYER_TUNING, SHOW_MARKED, indent);
    *t = w.t;
}

uint32_t patch_json_write(const uint8_t sound[LAB_SOUND_BYTES], const char* name, char* out,
                          uint32_t max) {
    text t = {out, 0, max};
    write_patch(&t, sound, name, 2);
    add_char(&t, '\n');
    return t.n + 1u < max ? t.n : 0u;
}

uint32_t patch_json_write_bank(const patch_bank* b, char* out, uint32_t max) {
    text t = {out, 0, max};
    add(&t, "{\n  \"format\": \"" PATCH_BANK_FORMAT "\",\n  \"name\": ");
    quoted(&t, b->name);
    add(&t, ",\n  \"patches\": [");
    for (uint32_t k = 0; k < b->count; k++) {
        add(&t, k ? ",\n    " : "\n    ");
        write_patch(&t, b->sounds[k], b->names[k], 6);
    }
    add(&t, "\n  ]\n}\n");
    return t.n + 1u < max ? t.n : 0u;
}

static const char ROLES[] = "auto\0drums\0bass\0arp\0lead\0none\0bass+drums\0acid\0";
static const char GATES[] = "staccato\0legato\0";
static const char INSTRUMENTS[] = "pulse\0bass\0kit\0";
static const char DRUMS[] = "none\0kick\0snare\0closed_hat\0open_hat\0clap\0low_tom\0high_tom\0rim\0"
                            "digi_kick\0digi_snare\0digi_clap\0digi_hat\0";
#define STEP_C3 36u
#define STEP_DRUMS 12u

enum { TOP_VOICE, TOP_PAGES, TOP_ROLE, TOP_GATE };
static const field STEP_TOP[] = {
    {"voice", K_INT, TOP_VOICE, 0xFF, F_PLUS1, 1, LIVE_VOICES, 0},
    {"pages", K_INT, TOP_PAGES, 0xFF, 0, 1, PATCH_STEP_PAGES, 0},
    {"role", K_ENUM, TOP_ROLE, 0xFF, 0, 0, 0, ROLES},
    {"gate", K_ENUM, TOP_GATE, 0xFF, 0, 0, 0, GATES},
    {0},
};

enum { CELL_STEP, CELL_NOTE, CELL_DRUM, CELL_SLIDE, CELL_ACCENT, CELL_CHANCE };
static const field STEP_CELL[] = {
    {"step", K_INT, CELL_STEP, 0xFF, F_PLUS1, 1, PATCH_STEP_MAX, 0},
    {"note", K_PITCH, CELL_NOTE, 0xFF, 0, 0, 0, 0},
    {"drum", K_ENUM, CELL_DRUM, 0xFF, 0, 0, 0, DRUMS},
    {"slide", K_BOOL, CELL_SLIDE, 0x01, 0, 0, 0, 0},
    {"accent", K_BOOL, CELL_ACCENT, 0x01, 0, 0, 0, 0},
    {"chance", K_INT, CELL_CHANCE, 0xFF, 0, 0, 100, 0},
    {0},
};


static int part_failed(reader* r, const char* error) {
    memcpy(r->err.out, error, PATCH_JSON_ERROR_MAX);
    r->err.n = length(r->err.out);
    return -1;
}

static int format_is(reader* r, const char* format, int* seen) {
    char got[24], quoted_format[24];
    text q = {quoted_format, 0, sizeof quoted_format};
    if (string(r, got, sizeof got) < 0) {
        return -1;
    }
    if (strcmp(got, format)) {
        add_char(&q, '"');
        add(&q, format);
        add_char(&q, '"');
        return bank_fail(r, "is not this format", quoted_format);
    }
    *seen = 1;
    return 0;
}

static int top_name(reader* r, char* out) {
    char name[64];
    int n = string(r, name, sizeof name);
    if (n < 0) {
        return -1;
    }
    if (n < 1 || n > (int)PATCH_BANK_NAME || name[0] == ' ') {
        return bank_fail(r, n > (int)PATCH_BANK_NAME ? "is too long" : "is not a name", BANK_NAME);
    }
    for (int k = 0; k <= n; k++) {
        char ch = name[k] >= 'a' && name[k] <= 'z' ? (char)(name[k] - 32) : name[k];
        if (k < n && (ch < NAME_FIRST || ch > NAME_LAST)) {
            return bank_fail(r, "has a character the FM-1 cannot show", BANK_NAME);
        }
        out[k] = ch;
    }
    return 0;
}

static int step_cells(reader* r, patch_step* st) {
    if (eat(r, '[', "a [")) {
        return -1;
    }
    while (peek(r) != ']') {
        if (st->count == PATCH_STEP_MAX) {
            return bank_fail(r, "has too many", "a list of up to 64 steps");
        }
        char prefix[16];
        text t = {prefix, 0, sizeof prefix};
        add(&t, "steps[");
        add_milli(&t, (int64_t)st->count * MILLI);
        add(&t, "]");
        uint32_t keep = r->path_n;
        r->path_n = 0;
        push(r, prefix, -1);
        uint8_t* c = st->scratch.sound;
        memset(c, 0, 8);
        memset(r->touched, 0, 8);
        c[CELL_NOTE] = STEP_C3;
        c[CELL_CHANCE] = 100;
        if (peek(r) != '{') {
            return fail(r, "must be an object { }", NULL);
        }
        if (object(r, STEP_CELL, 0, 0)) {
            return -1;
        }
        if (!(r->touched[CELL_STEP])) {
            return fail(r, "has no step. Allowed: \"step\": 1 to 64", NULL);
        }
        patch_step_cell* cell = &st->steps[st->count];
        cell->row = c[CELL_STEP];
        cell->note = c[CELL_NOTE];
        cell->drum = c[CELL_DRUM];
        cell->slide = c[CELL_SLIDE];
        cell->accent = c[CELL_ACCENT];
        cell->chance = c[CELL_CHANCE];
        for (uint32_t k = 0; k < st->count; k++) {
            if (st->steps[k].row == cell->row) {
                return fail(r, "is a step already given: one entry a step", NULL);
            }
        }
        st->count++;
        r->path_n = keep;
        r->path[keep] = 0;
        if (peek(r) == ',') {
            r->i++;
        } else if (peek(r) != ']') {
            return syntax(r, "a , or ]");
        }
    }
    r->i++;
    return 0;
}

static int step_instrument(reader* r, patch_step* st) {
    char c = peek(r);
    if (c == '{') {
        uint32_t start = r->i;
        if (skip_value(r)) {
            return -1;
        }
        if (read_part(&st->patch, r->s, start, r->i, "instrument", st->patch.error) < 0) {
            return part_failed(r, st->patch.error);
        }
        st->kind = PATCH_STEP_PATCH;
        return 0;
    }
    char word_in[16];
    if (c != '"') {
        return bank_fail(r, "must be a patch { } or text", "a patch, or \"pulse\", \"bass\" or \"kit\"");
    }
    if (string(r, word_in, sizeof word_in) < 0) {
        return -1;
    }
    int k = names_index(INSTRUMENTS, word_in);
    if (k < 0) {
        return bank_fail(r, "is not one", "a patch, or \"pulse\", \"bass\" or \"kit\"");
    }
    st->kind = (uint8_t)k;
    return 0;
}

static int step_checked(reader* r, patch_step* st, int format) {
    if (!format) {
        return whole_fail(r, "format", "is missing. Allowed: \"" PATCH_STEP_FORMAT "\"");
    }
    for (uint32_t k = 0; k < st->count; k++) {
        const patch_step_cell* c = &st->steps[k];
        char prefix[16];
        text t = {prefix, 0, sizeof prefix};
        add(&t, "steps[");
        add_milli(&t, (int64_t)k * MILLI);
        add(&t, "]");
        if (c->row >= st->pages * PATCH_STEP_ROWS) {
            return whole_fail(r, prefix, st->pages < PATCH_STEP_PAGES
                                             ? "is past the last page: add pages (16 steps each)"
                                             : "is past step 64");
        }
        if ((st->kind == PATCH_STEP_KIT) != (c->drum != 0)) {
            return whole_fail(r, prefix, st->kind == PATCH_STEP_KIT
                                             ? "needs a drum: the instrument is the kit"
                                             : "names a drum: a drum needs \"instrument\": \"kit\"");
        }
    }
    return 0;
}

int patch_json_read_step(patch_step* st, const char* s, uint32_t len) {
    reader r;
    int format = 0;
    reader_init(&r, &st->scratch, s, 0, len, st->error);
    memset(st->steps, 0, sizeof st->steps);
    st->count = 0;
    st->kind = PATCH_STEP_PULSE;
    memcpy(st->name, PATCH_STEP_NAME_DEFAULT, sizeof PATCH_STEP_NAME_DEFAULT);
    uint8_t top[4] = {0, 1, SONG_ROLE_AUTO, 0};
    while (r.i < r.n && s[r.i] != '{') {
        r.i++;
    }
    if (eat(&r, '{', "a { to start the pattern")) {
        return -1;
    }
    while (peek(&r) != '}') {
        char key[32];
        if (string(&r, key, sizeof key) < 0 || eat(&r, ':', "a :")) {
            return -1;
        }
        r.path_n = 0;
        push(&r, key, -1);
        const field* f = find(STEP_TOP, key);
        if (f) {
            memcpy(st->scratch.sound, top, sizeof top);
            if (value(&r, f, 0, 0)) {
                return -1;
            }
            memcpy(top, st->scratch.sound, sizeof top);
        } else if (!strcmp(key, "format")) {
            if (format_is(&r, PATCH_STEP_FORMAT, &format)) {
                return -1;
            }
        } else if (!strcmp(key, "name")) {
            if (top_name(&r, st->name)) {
                return -1;
            }
        } else if (!strcmp(key, "instrument")) {
            if (step_instrument(&r, st)) {
                return -1;
            }
        } else if (!strcmp(key, "steps")) {
            if (step_cells(&r, st)) {
                return -1;
            }
        } else {
            return fail(&r, "no such field. Allowed here: format, name, voice, role, gate, pages, "
                            "instrument, steps", NULL);
        }
        if (peek(&r) == ',') {
            r.i++;
        } else if (peek(&r) != '}') {
            return syntax(&r, "a , or }");
        }
    }
    st->voice = top[TOP_VOICE];
    st->pages = top[TOP_PAGES];
    st->role = top[TOP_ROLE];
    st->gate = top[TOP_GATE];
    return step_checked(&r, st, format);
}

#define SONG_FIELDS "format, name, bpm, instruments, sections, order"
#define SONG_LABEL "1 to 8 characters"
#define SONG_ORDER "a list of 1 to 64 section names"
#define SONG_INSTRUMENT "the name of one of instruments, or \"pulse\", \"bass\" or \"kit\""
#define SONG_BPM_DEFAULT 125u
#define SONG_NOTES "C0 to G#7, - . and ! or ~ after a note"
#define SONG_NOT_A_NOTE "is not a note. Allowed: " SONG_NOTES
#define SONG_NOT_A_DRUM                                                                             \
    "is not a drum. Allowed with the kit: k kick, s snare, h closed hat, o open hat, c clap, t low "  \
    "tom, T high tom, r rim, dk ds dc dh the digi kick, snare, clap and hat, - and ."
#define SONG_HOLD "-"
#define SONG_REST "."
#define SONG_ACCENT '!'
#define SONG_SLIDE '~'
#define SONG_TOKEN_MAX 8u
#define SONG_NOTE_MIN_CHARS 2u
#define SONG_NOTES_MAX (PATCH_SONG_ROWS * 12u)

static const char SONG_DRUMS[] = "k\0s\0h\0o\0c\0t\0T\0r\0dk\0ds\0dc\0dh\0";

static const char SONG_PARTS[] = "instruments\0sections\0order\0";
enum { PART_INSTRUMENTS, PART_SECTIONS, PART_ORDER, PARTS };
static const char VOICE_FIELDS[] = "instrument\0notes\0";
enum { VOICE_INSTRUMENT, VOICE_NOTES, VOICE_FIELD_COUNT };

static const field SONG_BPM = {"bpm", K_INT, 0, 0xFF, F_WIDE, PATCH_SONG_BPM_MIN, PATCH_SONG_BPM_MAX, 0};
static const field SONG_ROWS = {"rows", K_INT, 0, 0xFF, 0, 1, PATCH_SONG_ROWS, 0};

typedef int (*member_fn)(reader* r, const char* key, void* ctx);
typedef int (*visit_fn)(reader* r, void* ctx);

typedef struct {
    patch_song* sg;
    uint32_t at[PARTS];
    int format;
} song_read;

typedef struct {
    patch_song_section* sec;
    uint32_t voices_at;
} section_read;

typedef struct {
    const patch_song* sg;
    patch_song_section* sec;
    uint32_t voice;
    uint32_t at[VOICE_FIELD_COUNT];
} voice_read;

typedef struct {
    patch_song* sg;
    patch_song_section* sec;
} voices_read;

static void path_back(reader* r, uint32_t keep) {
    r->path_n = keep;
    r->path[keep] = 0;
}

static int members(reader* r, member_fn each, void* ctx) {
    if (eat(r, '{', "a {")) {
        return -1;
    }
    while (peek(r) != '}') {
        char key[32];
        uint32_t keep = r->path_n;
        if (string(r, key, sizeof key) < 0 || eat(r, ':', "a :")) {
            return -1;
        }
        push(r, key, -1);
        if (each(r, key, ctx)) {
            return -1;
        }
        path_back(r, keep);
        if (peek(r) == ',') {
            r->i++;
        } else if (peek(r) != '}') {
            return syntax(r, "a , or }");
        }
    }
    r->i++;
    return 0;
}

static int visit(reader* r, uint32_t at, const char* name, visit_fn read, void* ctx) {
    uint32_t end = r->i, keep = r->path_n;
    if (!at) {
        return 0;
    }
    r->i = at;
    push(r, name, -1);
    if (read(r, ctx)) {
        return -1;
    }
    r->i = end;
    path_back(r, keep);
    return 0;
}

static int deferred(reader* r, uint32_t* at) {
    peek(r);
    *at = r->i;
    return skip_value(r);
}

static int whole_value(reader* r, const field* f, uint32_t* out) {
    if (value(r, f, 0, 0)) {
        return -1;
    }
    *out = get(r->p->sound, f, 0, 0);
    return 0;
}

static int instrument_index(const patch_song* sg, const char* name) {
    for (uint32_t k = 0; k < sg->instruments; k++) {
        if (!strcmp(sg->labels[k], name)) {
            return (int)k;
        }
    }
    return -1;
}

static int section_index(const patch_song* sg, const char* name) {
    for (uint32_t k = 0; k < sg->sections; k++) {
        if (!strcmp(sg->section[k].label, name)) {
            return (int)k;
        }
    }
    return -1;
}

static int label_ok(reader* r, const char* key, int given) {
    uint32_t n = length(key);
    if (!n || n > PATCH_SONG_LABEL) {
        return bank_fail(r, n ? "is too long a name" : "is not a name", SONG_LABEL);
    }
    return given >= 0 ? fail(r, "is a name already given", NULL) : 0;
}

static void add_list(text* t, const char* const* names, uint32_t n) {
    for (uint32_t k = 0; k < n; k++) {
        add(t, k ? ", " : "");
        add(t, names[k]);
    }
}

static int not_named(reader* r, const char* what, const char* name, const char* listed,
                     const char* const* names, uint32_t n) {
    char w[PATCH_JSON_ERROR_MAX];
    text m = {w, 0, sizeof w};
    add(&m, "no ");
    add(&m, what);
    add(&m, " \"");
    add(&m, name);
    add(&m, "\". ");
    add(&m, listed);
    add_list(&m, names, n);
    return fail(r, w, NULL);
}

static int no_instrument(reader* r, const patch_song* sg, const char* name) {
    const char* names[PATCH_SONG_INSTRUMENTS + PATCH_STEP_PATCH];
    uint32_t n = 0;
    for (uint32_t k = 0; k < sg->instruments; k++) {
        names[n++] = sg->labels[k];
    }
    for (uint32_t b = 0; b < PATCH_STEP_PATCH; b++) {
        if (instrument_index(sg, nth(INSTRUMENTS, b)) < 0) {
            names[n++] = nth(INSTRUMENTS, b);
        }
    }
    for (uint32_t k = 1; k < n; k++) {
        for (uint32_t j = k; j && strcmp(names[j - 1u], names[j]) > 0; j--) {
            const char* swap = names[j];
            names[j] = names[j - 1u];
            names[j - 1u] = swap;
        }
    }
    return not_named(r, "instrument", name, "Instruments: ", names, n);
}

static int no_section(reader* r, const patch_song* sg, const char* name) {
    const char* names[PATCH_SONG_SECTIONS];
    for (uint32_t k = 0; k < sg->sections; k++) {
        names[k] = sg->section[k].label;
    }
    return not_named(r, "section", name, "Sections: ", names, sg->sections);
}

static int song_instrument(reader* r, const char* key, void* ctx) {
    patch_song* sg = ctx;
    if (sg->instruments == PATCH_SONG_INSTRUMENTS) {
        return whole_fail(r, "instruments", "has too many. Allowed: up to 16 patches");
    }
    if (label_ok(r, key, instrument_index(sg, key))) {
        return -1;
    }
    if (peek(r) != '{') {
        return fail(r, "must be a patch { }", NULL);
    }
    patch_json* p = &sg->patches[sg->instruments];
    uint32_t start = r->i;
    if (skip_value(r)) {
        return -1;
    }
    if (read_part(p, r->s, start, r->i, r->path, p->error) < 0) {
        return part_failed(r, p->error);
    }
    memcpy(sg->labels[sg->instruments++], key, length(key) + 1u);
    return 0;
}

static int song_instruments(reader* r, void* ctx) {
    if (peek(r) != '{') {
        return bank_fail(r, "must be an object", "up to 16 named patches, like \"lead\": {\"wave\": \"pulse\"}");
    }
    return members(r, song_instrument, ctx);
}

static int voice_instrument(reader* r, void* ctx) {
    voice_read* vr = ctx;
    char name[32];
    if (string(r, name, sizeof name) < 0) {
        return -1;
    }
    int k = instrument_index(vr->sg, name), b = names_index(INSTRUMENTS, name);
    if (k < 0 && b < 0) {
        return no_instrument(r, vr->sg, name);
    }
    vr->sec->instr[vr->voice] = (uint8_t)(k >= 0 ? k : (int)PATCH_SONG_BUILTIN + b);
    return 0;
}

static int separator(char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == ESCAPED_CONTROL;
}

static int token_fail(reader* r, uint32_t n, const char* token, const char* what) {
    char w[PATCH_JSON_ERROR_MAX];
    text m = {w, 0, sizeof w};
    add(&m, "token ");
    add_milli(&m, (int64_t)n * MILLI);
    add(&m, " \"");
    add(&m, token);
    add(&m, "\" ");
    add(&m, what);
    return fail(r, w, NULL);
}

static int note_cell(const char* token, patch_song_cell* c) {
    char base[SONG_TOKEN_MAX + 1u];
    uint32_t n = length(token);
    memcpy(base, token, n + 1u);
    while (n && (base[n - 1u] == SONG_ACCENT || base[n - 1u] == SONG_SLIDE)) {
        uint8_t* flag = base[n - 1u] == SONG_ACCENT ? &c->accent : &c->slide;
        if (*flag) {
            return -1;
        }
        *flag = 1;
        base[--n] = 0;
    }
    int pitch = n >= SONG_NOTE_MIN_CHARS ? pitch_of(base) : -1;
    c->note = (uint8_t)pitch;
    return pitch < 0 ? -1 : 0;
}

static int token_cell(reader* r, uint32_t n, const char* token, int kit, patch_song_cell* c) {
    int drum = names_index(SONG_DRUMS, token);
    memset(c, 0, sizeof *c);
    if (!strcmp(token, SONG_HOLD)) {
        c->hold = 1;
        return 0;
    }
    if (!strcmp(token, SONG_REST)) {
        c->rest = 1;
        return 0;
    }
    if (kit) {
        c->drum = (uint8_t)(drum + 1);
        c->note = STEP_C3;
        return drum < 0 ? token_fail(r, n, token, SONG_NOT_A_DRUM) : 0;
    }
    if (drum >= 0) {
        return token_fail(r, n, token, "is a drum, and drums need the kit. Allowed here: " SONG_NOTES);
    }
    return note_cell(token, c) ? token_fail(r, n, token, SONG_NOT_A_NOTE) : 0;
}

static int voice_notes(reader* r, void* ctx) {
    voice_read* vr = ctx;
    char notes[SONG_NOTES_MAX];
    int len = string(r, notes, sizeof notes);
    if (len < 0) {
        return -1;
    }
    if ((uint32_t)len >= sizeof notes) {
        return fail(r, "is too long. Allowed: one token a row, separated by spaces", NULL);
    }
    uint32_t rows = vr->sec->rows, count = 0;
    int kit = vr->sec->instr[vr->voice] == PATCH_SONG_BUILTIN + PATCH_STEP_KIT;
    patch_song_cell* cells = vr->sec->cells[vr->voice];
    patch_song_cell beyond;
    for (const char* at = notes; *at;) {
        char token[SONG_TOKEN_MAX + 1u];
        uint32_t n = 0;
        if (separator(*at)) {
            at++;
            continue;
        }
        for (; *at && !separator(*at); at++, n++) {
            token[n < SONG_TOKEN_MAX ? n : SONG_TOKEN_MAX] = *at;
        }
        token[n < SONG_TOKEN_MAX ? n : SONG_TOKEN_MAX] = 0;
        count++;
        if (n > SONG_TOKEN_MAX) {
            return token_fail(r, count, token, kit ? SONG_NOT_A_DRUM : SONG_NOT_A_NOTE);
        }
        if (token_cell(r, count, token, kit, count <= rows ? &cells[count - 1u] : &beyond)) {
            return -1;
        }
    }
    if (count != rows) {
        char w[48];
        text m = {w, 0, sizeof w};
        add_milli(&m, (int64_t)count * MILLI);
        add(&m, count == 1u ? " token, rows is " : " tokens, rows is ");
        add_milli(&m, (int64_t)rows * MILLI);
        return fail(r, w, NULL);
    }
    return 0;
}

static int voice_field(reader* r, const char* key, void* ctx) {
    voice_read* vr = ctx;
    int k = names_index(VOICE_FIELDS, key);
    if (k < 0) {
        return fail(r, "no such field. Allowed here: instrument, notes", NULL);
    }
    if (peek(r) != '"') {
        return fail(r, "must be text", NULL);
    }
    return deferred(r, &vr->at[k]);
}

static int song_voice(reader* r, voices_read* vs, uint32_t voice) {
    voice_read vr = {vs->sg, vs->sec, voice, {0}};
    if (peek(r) != '{') {
        return fail(r, "must be a voice { } or null", NULL);
    }
    if (members(r, voice_field, &vr)) {
        return -1;
    }
    if (!vr.at[VOICE_INSTRUMENT]) {
        return fail(r, "has no instrument. Allowed: " SONG_INSTRUMENT, NULL);
    }
    if (!vr.at[VOICE_NOTES]) {
        return fail(r, "has no notes. Allowed: one token a row, separated by spaces", NULL);
    }
    return visit(r, vr.at[VOICE_INSTRUMENT], "instrument", voice_instrument, &vr) ||
                   visit(r, vr.at[VOICE_NOTES], "notes", voice_notes, &vr)
               ? -1
               : 0;
}

static int section_voices(reader* r, void* ctx) {
    voices_read* vs = ctx;
    if (peek(r) != '[') {
        return bank_fail(r, "must be a list", "up to 3 voices, the first chip's");
    }
    r->i++;
    for (uint32_t v = 0; peek(r) != ']'; v++) {
        uint32_t keep = r->path_n;
        if (v == PATCH_SONG_VOICES) {
            return bank_fail(r, "has too many", "up to 3 voices, the first chip's");
        }
        push(r, NULL, (int)v);
        if (!(peek(r) == 'n' && word(r, "null")) && song_voice(r, vs, v)) {
            return -1;
        }
        path_back(r, keep);
        if (peek(r) == ',') {
            r->i++;
        } else if (peek(r) != ']') {
            return syntax(r, "a , or ]");
        }
    }
    r->i++;
    return 0;
}

static int section_field(reader* r, const char* key, void* ctx) {
    section_read* sr = ctx;
    uint32_t rows;
    if (!strcmp(key, "rows")) {
        if (whole_value(r, &SONG_ROWS, &rows)) {
            return -1;
        }
        sr->sec->rows = (uint8_t)rows;
        return 0;
    }
    if (!strcmp(key, "voices")) {
        return deferred(r, &sr->voices_at);
    }
    return fail(r, "no such field. Allowed here: rows, voices", NULL);
}

static int song_section(reader* r, const char* key, void* ctx) {
    patch_song* sg = ctx;
    if (sg->sections == PATCH_SONG_SECTIONS) {
        return whole_fail(r, "sections", "has too many. Allowed: up to 16 sections");
    }
    if (label_ok(r, key, section_index(sg, key))) {
        return -1;
    }
    patch_song_section* sec = &sg->section[sg->sections];
    section_read sr = {sec, 0};
    voices_read vs = {sg, sec};
    memset(sec, 0, sizeof *sec);
    memset(sec->instr, PATCH_SONG_REST_VOICE, sizeof sec->instr);
    if (peek(r) != '{') {
        return fail(r, "must be an object { }", NULL);
    }
    if (members(r, section_field, &sr)) {
        return -1;
    }
    if (!sec->rows) {
        return fail(r, "has no rows. Allowed: \"rows\": 1 to 128", NULL);
    }
    if (visit(r, sr.voices_at, "voices", section_voices, &vs)) {
        return -1;
    }
    memcpy(sec->label, key, length(key) + 1u);
    sg->sections++;
    return 0;
}

static int song_sections(reader* r, void* ctx) {
    if (peek(r) != '{') {
        return bank_fail(r, "must be an object", "up to 16 named sections, like \"A\": {\"rows\": 16, \"voices\": []}");
    }
    return members(r, song_section, ctx);
}

static int song_order(reader* r, void* ctx) {
    patch_song* sg = ctx;
    if (peek(r) != '[') {
        return bank_fail(r, "must be a list", SONG_ORDER);
    }
    r->i++;
    while (peek(r) != ']') {
        uint32_t keep = r->path_n;
        char name[32];
        if (sg->order_len == PATCH_SONG_ORDER) {
            return bank_fail(r, "has too many", SONG_ORDER);
        }
        push(r, NULL, (int)sg->order_len);
        if (peek(r) != '"') {
            return fail(r, "must be a section's name, as text", NULL);
        }
        if (string(r, name, sizeof name) < 0) {
            return -1;
        }
        int k = section_index(sg, name);
        if (k < 0) {
            return no_section(r, sg, name);
        }
        sg->order[sg->order_len++] = (uint8_t)k;
        path_back(r, keep);
        if (peek(r) == ',') {
            r->i++;
        } else if (peek(r) != ']') {
            return syntax(r, "a , or ]");
        }
    }
    r->i++;
    return 0;
}

static int song_top(reader* r, const char* key, void* ctx) {
    song_read* sr = ctx;
    uint32_t bpm;
    int part = names_index(SONG_PARTS, key);
    if (part >= 0) {
        return deferred(r, &sr->at[part]);
    }
    if (!strcmp(key, "format")) {
        return format_is(r, PATCH_SONG_FORMAT, &sr->format);
    }
    if (!strcmp(key, "name")) {
        return top_name(r, sr->sg->name);
    }
    if (!strcmp(key, "bpm")) {
        if (whole_value(r, &SONG_BPM, &bpm)) {
            return -1;
        }
        sr->sg->bpm = (uint16_t)bpm;
        return 0;
    }
    return fail(r, "no such field. Allowed here: " SONG_FIELDS, NULL);
}

int patch_json_read_song(patch_song* sg, const char* s, uint32_t len) {
    patch_json scratch;
    song_read sr = {sg, {0}, 0};
    reader r;
    reader_init(&r, &scratch, s, 0, len, sg->error);
    sg->sections = sg->order_len = sg->instruments = 0;
    sg->bpm = SONG_BPM_DEFAULT;
    sg->bpm_played10 = 0;
    memcpy(sg->name, PATCH_SONG_NAME_DEFAULT, sizeof PATCH_SONG_NAME_DEFAULT);
    while (r.i < r.n && s[r.i] != '{') {
        r.i++;
    }
    if (r.i == r.n) {
        return syntax(&r, "a { to start the song");
    }
    if (members(&r, song_top, &sr)) {
        return -1;
    }
    if (!sr.format) {
        return whole_fail(&r, "format", "is missing. Allowed: \"" PATCH_SONG_FORMAT "\"");
    }
    if (visit(&r, sr.at[PART_INSTRUMENTS], "instruments", song_instruments, sg) ||
        visit(&r, sr.at[PART_SECTIONS], "sections", song_sections, sg)) {
        return -1;
    }
    if (!sg->sections) {
        return whole_fail(&r, "sections", "is empty. Allowed: 1 to 16 named sections");
    }
    if (visit(&r, sr.at[PART_ORDER], "order", song_order, sg)) {
        return -1;
    }
    if (!sg->order_len) {
        return whole_fail(&r, "order", "is empty. Allowed: " SONG_ORDER);
    }
    return PATCH_JSON_OK;
}

int patch_json_kind(const char* s, uint32_t len) {
    char error[PATCH_JSON_ERROR_MAX];
    char format[24];
    reader r;
    reader_init(&r, NULL, s, 0, len, error);
    if (top_format(&r, format, sizeof format)) {
        return PATCH_JSON_PATCH_TEXT;
    }
    return !strcmp(format, PATCH_BANK_FORMAT)   ? PATCH_JSON_BANK_TEXT
           : !strcmp(format, PATCH_STEP_FORMAT) ? PATCH_JSON_STEP_TEXT
           : !strcmp(format, PATCH_SONG_FORMAT) ? PATCH_JSON_SONG_TEXT
                                                : PATCH_JSON_PATCH_TEXT;
}

static uint32_t template_read(void* ctx, uint32_t off, uint8_t* p, uint32_t n) {
    return song_io_template(ctx, off, p, n);
}

static int edited(gt2_song* s, gt2_err e, gt2_edit* ed) {
    if (e != GT2_OK) {
        return -1;
    }
    gt2_edit_commit(s, ed);
    return 0;
}

static uint32_t lane_pattern(const gt2_song* s, uint32_t voice) {
    const uint8_t* o = gt2_order_data(s, 0, voice);
    uint32_t n = gt2_order_entries(s, 0, voice);
    for (uint32_t k = 0; o && k < n; k++) {
        if (o[k] < GT2_MAX_PATTERNS) {
            return o[k];
        }
    }
    return GT2_MAX_PATTERNS;
}

static int place_patch(gt2_song* s, const patch_json* p, uint32_t model, uint32_t* instr) {
    lab_song_instr in;
    uint8_t first[GT2_TABLES];
    uint32_t i = lab_song_free(s);
    if (!i) {
        return -1;
    }
    for (uint32_t t = 0; t < GT2_TABLES; t++) {
        first[t] = (uint8_t)(lab_song_table_end(s, t) + 1u);
    }
    lab_song_encode(p->sound, p->name, 0, model, first, &in);
    for (uint32_t t = 0; t < GT2_TABLES; t++) {
        if (first[t] - 1u + in.rows[t] > GT2_TABLE_ROWS) {
            return -1;
        }
        memcpy(&s->ltable[t][first[t] - 1u], in.left[t], in.rows[t]);
        memcpy(&s->rtable[t][first[t] - 1u], in.right[t], in.rows[t]);
    }
    memcpy(s->instr[i - 1u], in.instr, GT2_INSTR_BYTES);
    gt2_saved_recount(s);
    *instr = i;
    return lab_song_keep(s, i, &in, p->sound);
}

static int template_song(gt2_song* s) {
    gt2_caps caps = {SONG_MULTISPEED_MAX};
    gt2_read_info info;
    memset(&info, 0, sizeof info);
    return gt2_sng_read(s, template_read, NULL, &caps, &info) ? -1 : 0;
}

int patch_json_step_song(gt2_song* s, patch_step* st, uint32_t model) {
    gt2_edit e;
    if (template_song(s)) {
        return -1;
    }
    uint32_t p = lane_pattern(s, st->voice), rows = st->pages * PATCH_STEP_ROWS;
    uint32_t instr = st->kind == PATCH_STEP_BASS ? 2u : 1u;
    st->instr = 0;
    if (p >= GT2_MAX_PATTERNS) {
        return -1;
    }
    memset(&e, 0, sizeof e);
    if (gt2_pattern_rows(s, p) - 1u != rows && edited(s, gt2_pattern_resize(s, p, rows, &e), &e)) {
        return -1;
    }
    if (st->kind == PATCH_STEP_PATCH) {
        if (place_patch(s, &st->patch, model, &instr)) {
            return -1;
        }
        st->instr = (uint8_t)instr;
    }
    uint8_t cells[PATCH_STEP_MAX][4];
    for (uint32_t r = 0; r < rows; r++) {
        cells[r][0] = GT2_REST;
        cells[r][1] = cells[r][2] = cells[r][3] = 0;
    }
    for (uint32_t k = 0; k < st->count; k++) {
        const patch_step_cell* c = &st->steps[k];
        cells[c->row][0] = (uint8_t)(GT2_FIRSTNOTE + c->note);
        cells[c->row][1] = (uint8_t)(c->drum ? SONG_IO_KIT + c->drum - 1u : instr);
    }
    for (uint32_t r = 0; !st->gate && r + 1u < rows; r++) {
        if (cells[r][0] < GT2_REST && cells[r + 1u][0] == GT2_REST) {
            cells[r + 1u][0] = GT2_KEYOFF;
        }
    }
    for (uint32_t r = 0; r < rows; r++) {
        memset(&e, 0, sizeof e);
        if (edited(s, gt2_set_row(s, p, r, cells[r], &e), &e)) {
            return -1;
        }
    }
    for (uint32_t k = 0; k < st->count; k++) {
        const patch_step_cell* c = &st->steps[k];
        if ((c->slide && song_lock_put(s, p, c->row, SONG_LOCK_SLIDE, 1u)) ||
            (c->accent && song_lock_put(s, p, c->row, SONG_LOCK_ACCENT, 1u)) ||
            (c->chance < 100u && song_lock_put(s, p, c->row, SONG_LOCK_CHANCE, c->chance))) {
            return -1;
        }
    }
    song_scene sc;
    if (!song_ext_get_scene(s, &sc)) {
        memset(&sc, 0, sizeof sc);
        sc.count = LIVE_VOICES;
    }
    sc.role[st->voice] = st->role;
    memset(s->name, 0, sizeof s->name);
    memcpy(s->name, st->name, length(st->name));
    gt2_saved_recount(s);
    return song_ext_set_scene(s, &sc) ? -1 : 0;
}

#define SONG_NEW_PULSE 1u
#define SONG_NEW_BASS 2u
#define SONG_UNMADE 0xFFu
#define SONG_SPEED_MIN 3u
#define SONG_SPEED_MAX 32u
#define SONG_NEW_SPEED 6u
#define SONG_ROWS_A_BEAT 4u
#define SONG_ONE_CALL_A_FRAME 1u
#define SONG_PAL 0u
#define SONG_FUNKTEMPO 0x0Eu
#define SONG_FULL                                                                                  \
    "SONG FULL here: a song holds 208 patterns, 64 KiB and 512 accents and slides. Use fewer or "  \
    "shorter sections, or fewer accents and slides"
#define SONG_TABLES_FULL                                                                           \
    "the song's tables are full here: a song holds 255 rows of each table. Drop a patch or "      \
    "shorten its tables"

typedef struct {
    uint32_t rows;
    uint8_t row[PATCH_SONG_ROWS][GT2_ROW_BYTES];
    uint8_t accent[PATCH_SONG_ROWS], slide[PATCH_SONG_ROWS];
} song_lane;

typedef struct {
    const patch_song* sg;
    uint8_t instr[PATCH_SONG_BUILTIN + PATCH_STEP_PATCH];
    uint8_t made[PATCH_SONG_SECTIONS][PATCH_SONG_VOICES];
    uint8_t from[GT2_MAX_PATTERNS][2];
    uint32_t patterns;
    uint32_t tempo_row;
} song_build;

static int song_fail(patch_song* sg, const char* part, const char* label, int voice, const char* what) {
    text t = {sg->error, 0, PATCH_JSON_ERROR_MAX};
    add(&t, part);
    if (label) {
        add_char(&t, '.');
        add(&t, label);
    }
    if (voice >= 0) {
        add(&t, ".voices[");
        add_milli(&t, (int64_t)voice * MILLI);
        add_char(&t, ']');
    }
    add(&t, ": ");
    add(&t, what);
    add_char(&t, '.');
    return -1;
}

typedef struct {
    uint32_t first, second, bpm10;
} song_speeds;

static uint32_t speeds_bpm10(uint32_t speed_sum) {
    return song_tempo_bpm10(speed_sum, SONG_ONE_CALL_A_FRAME, SONG_PAL, SONG_ROWS_A_BEAT);
}

static song_speeds speeds_of(uint32_t bpm) {
    song_speeds best = {SONG_NEW_SPEED, SONG_NEW_SPEED, speeds_bpm10(2u * SONG_NEW_SPEED)};
    uint32_t want = bpm * 10u;
    for (uint32_t a = SONG_SPEED_MIN; a <= SONG_SPEED_MAX; a++) {
        for (uint32_t b = a; b <= a + 1u; b++) {
            uint32_t got = speeds_bpm10(a + b);
            uint32_t gap = got > want ? got - want : want - got;
            uint32_t best_gap = best.bpm10 > want ? best.bpm10 - want : want - best.bpm10;
            if (gap < best_gap) {
                best = (song_speeds){a, b, got};
            }
        }
    }
    return best;
}

static int song_tempo(gt2_song* s, song_build* b, patch_song* sg) {
    song_speeds sp = speeds_of(sg->bpm);
    sg->bpm_played10 = (uint16_t)sp.bpm10;
    if (sp.first == SONG_NEW_SPEED && sp.second == SONG_NEW_SPEED) {
        return 0;
    }
    b->tempo_row = lab_song_table_end(s, GT2_STBL) + 1u;
    if (b->tempo_row > GT2_TABLE_ROWS) {
        return song_fail(sg, "bpm", NULL, -1, SONG_TABLES_FULL);
    }
    s->ltable[GT2_STBL][b->tempo_row - 1u] = (uint8_t)sp.first;
    s->rtable[GT2_STBL][b->tempo_row - 1u] = (uint8_t)sp.second;
    return 0;
}

static void lane_of(const song_build* b, uint32_t section, uint32_t voice, song_lane* lane) {
    const patch_song_section* sec = &b->sg->section[section];
    uint32_t instr = sec->instr[voice];
    memset(lane, 0, sizeof *lane);
    lane->rows = sec->rows;
    for (uint32_t r = 0; r < sec->rows; r++) {
        const patch_song_cell* c = &sec->cells[voice][r];
        uint8_t* row = lane->row[r];
        if (instr == PATCH_SONG_REST_VOICE) {
            row[0] = r ? GT2_REST : GT2_KEYOFF;
        } else if (c->hold || c->rest) {
            row[0] = c->hold ? GT2_REST : GT2_KEYOFF;
        } else {
            row[0] = (uint8_t)(GT2_FIRSTNOTE + c->note);
            row[1] = (uint8_t)(c->drum ? SONG_IO_KIT + c->drum - 1u : b->instr[instr]);
            lane->accent[r] = c->accent;
            lane->slide[r] = c->slide;
        }
    }
    if (b->tempo_row && section == b->sg->order[0] && voice == 0u) {
        lane->row[0][2] = SONG_FUNKTEMPO;
        lane->row[0][3] = (uint8_t)b->tempo_row;
    }
}

static int lane_made(const song_build* b, const song_lane* lane, uint32_t* pattern) {
    song_lane other;
    for (uint32_t p = 0; p < b->patterns; p++) {
        lane_of(b, b->from[p][0], b->from[p][1], &other);
        if (!memcmp(&other, lane, sizeof other)) {
            *pattern = p;
            return 1;
        }
    }
    return 0;
}

static int lane_write(gt2_song* s, const song_lane* lane, uint32_t* pattern) {
    gt2_edit e;
    memset(&e, 0, sizeof e);
    if (edited(s, gt2_pattern_add(s, lane->rows, pattern, &e), &e)) {
        return -1;
    }
    for (uint32_t r = 0; r < lane->rows; r++) {
        memset(&e, 0, sizeof e);
        if (edited(s, gt2_set_row(s, *pattern, r, lane->row[r], &e), &e) ||
            (lane->accent[r] && song_lock_put(s, *pattern, r, SONG_LOCK_ACCENT, 1u)) ||
            (lane->slide[r] && song_lock_put(s, *pattern, r, SONG_LOCK_SLIDE, 1u))) {
            return -1;
        }
    }
    return 0;
}

static int section_patterns(gt2_song* s, song_build* b, patch_song* sg, uint32_t section) {
    song_lane lane;
    for (uint32_t v = 0; v < PATCH_SONG_VOICES; v++) {
        uint32_t p = 0;
        lane_of(b, section, v, &lane);
        if (!lane_made(b, &lane, &p)) {
            if (lane_write(s, &lane, &p) || p != b->patterns) {
                return song_fail(sg, "sections", sg->section[section].label, (int)v, SONG_FULL);
            }
            b->from[p][0] = (uint8_t)section;
            b->from[p][1] = (uint8_t)v;
            b->patterns++;
        }
        b->made[section][v] = (uint8_t)p;
    }
    return 0;
}

static int template_cleared(gt2_song* s) {
    gt2_edit e;
    for (uint32_t v = 0; v < PATCH_SONG_VOICES; v++) {
        memset(&e, 0, sizeof e);
        if (edited(s, gt2_order_replace(s, 0, v, NULL, 0, 0, &e), &e)) {
            return -1;
        }
    }
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        memset(&e, 0, sizeof e);
        if (gt2_pattern_data(s, p) && edited(s, gt2_pattern_remove(s, p, &e), &e)) {
            return -1;
        }
    }
    return 0;
}

static int song_instruments_placed(gt2_song* s, song_build* b, patch_song* sg, uint32_t model) {
    b->instr[PATCH_SONG_BUILTIN + PATCH_STEP_PULSE] = SONG_NEW_PULSE;
    b->instr[PATCH_SONG_BUILTIN + PATCH_STEP_BASS] = SONG_NEW_BASS;
    for (uint32_t k = 0; k < sg->instruments; k++) {
        uint32_t instr = 0;
        if (place_patch(s, &sg->patches[k], model, &instr)) {
            return song_fail(sg, "instruments", sg->labels[k], -1, SONG_TABLES_FULL);
        }
        b->instr[k] = (uint8_t)instr;
    }
    return 0;
}

int patch_json_song_song(gt2_song* s, patch_song* sg, uint32_t model) {
    song_build b;
    uint8_t entries[PATCH_SONG_ORDER];
    gt2_edit e;
    memset(&b, 0, sizeof b);
    memset(b.made, SONG_UNMADE, sizeof b.made);
    b.sg = sg;
    sg->error[0] = 0;
    if (template_song(s) || song_instruments_placed(s, &b, sg, model) || song_tempo(s, &b, sg)) {
        return -1;
    }
    if (template_cleared(s)) {
        return song_fail(sg, "sections", NULL, -1, SONG_FULL);
    }
    for (uint32_t k = 0; k < sg->order_len; k++) {
        if (b.made[sg->order[k]][0] == SONG_UNMADE && section_patterns(s, &b, sg, sg->order[k])) {
            return -1;
        }
    }
    for (uint32_t v = 0; v < PATCH_SONG_VOICES; v++) {
        for (uint32_t k = 0; k < sg->order_len; k++) {
            entries[k] = b.made[sg->order[k]][v];
        }
        memset(&e, 0, sizeof e);
        if (edited(s, gt2_order_replace(s, 0, v, entries, sg->order_len, 0, &e), &e)) {
            return song_fail(sg, "order", NULL, -1, SONG_FULL);
        }
    }
    memset(s->name, 0, sizeof s->name);
    memcpy(s->name, sg->name, length(sg->name));
    gt2_saved_recount(s);
    return 0;
}

static void kv(text* t, const char* key, const char* v, int quote) {
    add(t, ",\n  \"");
    add(t, key);
    add(t, "\": ");
    if (quote) {
        quoted(t, v);
    } else {
        add(t, v);
    }
}

uint32_t patch_json_write_step(const gt2_song* s, uint32_t voice, char* out, uint32_t max) {
    uint32_t p = lane_pattern(s, voice);
    if (voice >= LIVE_VOICES || p >= GT2_MAX_PATTERNS) {
        return 0;
    }
    uint32_t rows = gt2_pattern_rows(s, p) - 1u;
    const uint8_t* d = gt2_pattern_data(s, p);
    if (!d || rows % PATCH_STEP_ROWS || rows > PATCH_STEP_MAX) {
        return 0;
    }
    uint32_t kind = PATCH_STEP_PULSE, instr = 0, stac = 0;
    for (uint32_t r = 0; r < rows; r++) {
        const uint8_t* c = d + r * GT2_ROW_BYTES;
        if (c[0] >= GT2_REST) {
            stac |= c[0] == GT2_KEYOFF;
            continue;
        }
        uint32_t i = c[1], drum = i >= SONG_IO_KIT && i < SONG_IO_KIT + STEP_DRUMS;
        if (instr && (drum ? kind != PATCH_STEP_KIT : i != instr)) {
            return 0;
        }
        instr = drum ? instr ? instr : i : i;
        kind = drum ? PATCH_STEP_KIT : i == 1u ? PATCH_STEP_PULSE : i == 2u ? PATCH_STEP_BASS : PATCH_STEP_PATCH;
    }
    uint8_t sound[LAB_SOUND_BYTES], kept_rows[GT2_TABLES];
    char name[LAB_SOUND_NAME + 1u];
    if (kind == PATCH_STEP_PATCH && !lab_song_kept(s, instr, kept_rows, sound)) {
        return 0;
    }
    song_scene sc;
    uint32_t role = song_ext_get_scene(s, &sc) && voice < sc.count ? sc.role[voice] : SONG_ROLE_AUTO;
    char num[8], song_name[PATCH_BANK_NAME + 1u];
    text t = {out, 0, max}, nt = {num, 0, sizeof num};
    memset(song_name, 0, sizeof song_name);
    for (uint32_t k = 0; k < PATCH_BANK_NAME && s->name[k]; k++) {
        song_name[k] = s->name[k];
    }
    add(&t, "{\n  \"format\": \"" PATCH_STEP_FORMAT "\"");
    kv(&t, "name", song_name[0] ? song_name : PATCH_STEP_NAME_DEFAULT, 1);
    add_char(&nt, (char)('1' + voice));
    kv(&t, "voice", num, 0);
    kv(&t, "role", nth(ROLES, role % SONG_ROLES), 1);
    kv(&t, "gate", stac ? "staccato" : "legato", 1);
    nt.n = 0;
    add_milli(&nt, (int64_t)(rows / PATCH_STEP_ROWS) * MILLI);
    kv(&t, "pages", num, 0);
    if (kind == PATCH_STEP_PATCH) {
        lab_song_name(s, instr, name);
        add(&t, ",\n  \"instrument\": ");
        write_patch(&t, sound, name, 4);
    } else {
        kv(&t, "instrument", nth(INSTRUMENTS, kind), 1);
    }
    add(&t, ",\n  \"steps\": [");
    for (uint32_t r = 0, first = 1; r < rows; r++) {
        const uint8_t* c = d + r * GT2_ROW_BYTES;
        if (c[0] >= GT2_REST) {
            continue;
        }
        uint32_t v = 0;
        add(&t, first ? "\n    {\"step\": " : ",\n    {\"step\": ");
        first = 0;
        add_milli(&t, (int64_t)(r + 1u) * MILLI);
        uint32_t note = c[0] - GT2_FIRSTNOTE;
        if (note != STEP_C3) {
            char nn[5];
            text nb = {nn, 0, sizeof nn};
            add(&nb, nth(NOTE_NAMES, note % 12u));
            add_char(&nb, (char)('0' + note / 12u));
            add(&t, ", \"note\": ");
            quoted(&t, nn);
        }
        if (kind == PATCH_STEP_KIT) {
            add(&t, ", \"drum\": ");
            quoted(&t, nth(DRUMS, c[1] - SONG_IO_KIT + 1u));
        }
        if (song_lock_get(s, p, r, SONG_LOCK_SLIDE, &v) && v) {
            add(&t, ", \"slide\": true");
        }
        if (song_lock_get(s, p, r, SONG_LOCK_ACCENT, &v) && v) {
            add(&t, ", \"accent\": true");
        }
        if (song_lock_get(s, p, r, SONG_LOCK_CHANCE, &v) && v < 100u) {
            add(&t, ", \"chance\": ");
            add_milli(&t, (int64_t)v * MILLI);
        }
        add_char(&t, '}');
    }
    add(&t, "\n  ]\n}\n");
    return t.n + 1u < max ? t.n : 0u;
}

static const char* json_type(const field* f) {
    static const char* const KINDS[] = {"integer", "number", "boolean", "enum",   "set",
                                        "voices",  "number", "number",  "note",   "name",
                                        "format",  "object", "object",  "list",   "pitch"};
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

static int place_all(gt2_song* s, const uint8_t (*sounds)[LAB_SOUND_BYTES], const char (*names)[LAB_SOUND_NAME + 1u],
                     uint32_t count, const char* song_name, uint32_t model) {
    lab_song_instr in[PATCH_BANK_MAX];
    uint32_t end[GT2_TABLES] = {0};
    if (gt2_song_clear(s)) {
        return -1;
    }
    for (uint32_t k = 0; k < count; k++) {
        uint8_t first[GT2_TABLES];
        for (uint32_t t = 0; t < GT2_TABLES; t++) {
            first[t] = (uint8_t)(end[t] + 1u);
        }
        lab_song_encode(sounds[k], names[k], 0, model, first, &in[k]);
        for (uint32_t t = 0; t < GT2_TABLES; t++) {
            if (end[t] + in[k].rows[t] > GT2_TABLE_ROWS) {
                return (int)(k + 1u);
            }
            memcpy(&s->ltable[t][end[t]], in[k].left[t], in[k].rows[t]);
            memcpy(&s->rtable[t][end[t]], in[k].right[t], in[k].rows[t]);
            end[t] += in[k].rows[t];
        }
        memcpy(s->instr[k], in[k].instr, GT2_INSTR_BYTES);
    }
    memset(s->name, 0, sizeof s->name);
    for (uint32_t k = 0; k < GT2_STR - 1u && song_name[k]; k++) {
        s->name[k] = song_name[k];
    }
    gt2_saved_recount(s);
    for (uint32_t k = 0; k < count; k++) {
        if (lab_song_keep(s, k + 1u, &in[k], sounds[k])) {
            return -1;
        }
    }
    return 0;
}

int patch_json_song(gt2_song* s, const uint8_t sound[LAB_SOUND_BYTES], const char* name,
                    uint32_t model) {
    char names[1][LAB_SOUND_NAME + 1u] = {{0}};
    for (uint32_t k = 0; k < LAB_SOUND_NAME && name[k]; k++) {
        names[0][k] = name[k];
    }
    return place_all(s, (const uint8_t(*)[LAB_SOUND_BYTES])sound, (const char(*)[LAB_SOUND_NAME + 1u])names, 1u, names[0], model);
}

int patch_json_bank_song(gt2_song* s, patch_bank* b, uint32_t model) {
    int r = place_all(s, (const uint8_t(*)[LAB_SOUND_BYTES])b->sounds, (const char(*)[LAB_SOUND_NAME + 1u])b->names, b->count, b->name, model);
    if (r > 0) {
        text t = {b->error, 0, sizeof b->error};
        add(&t, "patches[");
        add_milli(&t, (int64_t)(r - 1) * MILLI);
        add(&t, "]: the bank's tables are full here: a song holds 255 rows of each table. Drop a patch or shorten the tables.");
    }
    return r;
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
