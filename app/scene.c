// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "scene.h"

#include <stddef.h>
#include <string.h>

#include "font.h"

/* A blank cell: black on black. */
#define BLANK_CH ((uint8_t)' ')
#define BLANK_ATTR GRID_ATTR(GRID_BLACK, GRID_BLACK)

const char* const scene_names[SCENE_PIECES] = {"FIRE", "PLASMA", "RULE 30", "STARS", "ANT"};

const uint8_t scene_dist4[GRID_ROWS][GRID_COLS] = {
    {0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56,
     60, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116},
    {4, 6, 9, 13, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56,
     60, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116},
    {8, 9, 11, 14, 18, 22, 25, 29, 33, 37, 41, 45, 49, 53, 57,
     61, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116},
    {12, 13, 14, 17, 20, 23, 27, 30, 34, 38, 42, 46, 49, 53, 57,
     61, 65, 69, 73, 77, 81, 85, 89, 93, 97, 101, 105, 109, 113, 117},
    {16, 16, 18, 20, 23, 26, 29, 32, 36, 39, 43, 47, 51, 54, 58,
     62, 66, 70, 74, 78, 82, 86, 89, 93, 97, 101, 105, 109, 113, 117},
    {20, 20, 22, 23, 26, 28, 31, 34, 38, 41, 45, 48, 52, 56, 59,
     63, 67, 71, 75, 79, 82, 86, 90, 94, 98, 102, 106, 110, 114, 118},
    {24, 24, 25, 27, 29, 31, 34, 37, 40, 43, 47, 50, 54, 57, 61,
     65, 68, 72, 76, 80, 84, 87, 91, 95, 99, 103, 107, 111, 115, 118},
    {28, 28, 29, 30, 32, 34, 37, 40, 43, 46, 49, 52, 56, 59, 63,
     66, 70, 74, 77, 81, 85, 89, 92, 96, 100, 104, 108, 112, 115, 119},
    {32, 32, 33, 34, 36, 38, 40, 43, 45, 48, 51, 54, 58, 61, 64,
     68, 72, 75, 79, 82, 86, 90, 94, 97, 101, 105, 109, 113, 116, 120},
    {36, 36, 37, 38, 39, 41, 43, 46, 48, 51, 54, 57, 60, 63, 67,
     70, 73, 77, 80, 84, 88, 91, 95, 99, 103, 106, 110, 114, 118, 121},
    {40, 40, 41, 42, 43, 45, 47, 49, 51, 54, 57, 59, 62, 66, 69,
     72, 75, 79, 82, 86, 89, 93, 97, 100, 104, 108, 111, 115, 119, 123},
    {44, 44, 45, 46, 47, 48, 50, 52, 54, 57, 59, 62, 65, 68, 71,
     74, 78, 81, 84, 88, 91, 95, 98, 102, 106, 109, 113, 117, 120, 124},
    {48, 48, 49, 49, 51, 52, 54, 56, 58, 60, 62, 65, 68, 71, 74,
     77, 80, 83, 87, 90, 93, 97, 100, 104, 107, 111, 115, 118, 122, 126},
    {52, 52, 53, 53, 54, 56, 57, 59, 61, 63, 66, 68, 71, 74, 76,
     79, 82, 86, 89, 92, 95, 99, 102, 106, 109, 113, 116, 120, 123, 127},
    {56, 56, 57, 57, 58, 59, 61, 63, 64, 67, 69, 71, 74, 76, 79,
     82, 85, 88, 91, 94, 98, 101, 104, 108, 111, 115, 118, 122, 125, 129},
    {60, 60, 61, 61, 62, 63, 65, 66, 68, 70, 72, 74, 77, 79, 82,
     85, 88, 91, 94, 97, 100, 103, 107, 110, 113, 117, 120, 124, 127, 131},
    {64, 64, 64, 65, 66, 67, 68, 70, 72, 73, 75, 78, 80, 82, 85,
     88, 91, 93, 96, 99, 102, 106, 109, 112, 115, 119, 122, 126, 129, 132},
    {68, 68, 68, 69, 70, 71, 72, 74, 75, 77, 79, 81, 83, 86, 88,
     91, 93, 96, 99, 102, 105, 108, 111, 114, 118, 121, 124, 128, 131, 134},
    {72, 72, 72, 73, 74, 75, 76, 77, 79, 80, 82, 84, 87, 89, 91,
     94, 96, 99, 102, 105, 108, 111, 114, 117, 120, 123, 126, 130, 133, 137},
    {76, 76, 76, 77, 78, 79, 80, 81, 82, 84, 86, 88, 90, 92, 94,
     97, 99, 102, 105, 107, 110, 113, 116, 119, 122, 126, 129, 132, 135, 139},
    {80, 80, 80, 81, 82, 82, 84, 85, 86, 88, 89, 91, 93, 95, 98,
     100, 102, 105, 108, 110, 113, 116, 119, 122, 125, 128, 131, 134, 138, 141},
    {84, 84, 84, 85, 86, 86, 87, 89, 90, 91, 93, 95, 97, 99, 101,
     103, 106, 108, 111, 113, 116, 119, 122, 125, 128, 131, 134, 137, 140, 143},
    {88, 88, 88, 89, 89, 90, 91, 92, 94, 95, 97, 98, 100, 102, 104,
     107, 109, 111, 114, 116, 119, 122, 124, 127, 130, 133, 136, 139, 142, 146},
    {92, 92, 92, 93, 93, 94, 95, 96, 97, 99, 100, 102, 104, 106, 108,
     110, 112, 114, 117, 119, 122, 125, 127, 130, 133, 136, 139, 142, 145, 148},
    {96, 96, 96, 97, 97, 98, 99, 100, 101, 103, 104, 106, 107, 109, 111,
     113, 115, 118, 120, 122, 125, 128, 130, 133, 136, 139, 142, 144, 148, 151},
    {100, 100, 100, 101, 101, 102, 103, 104, 105, 106, 108, 109, 111, 113, 115,
     117, 119, 121, 123, 126, 128, 131, 133, 136, 139, 141, 144, 147, 150, 153},
    {104, 104, 104, 105, 105, 106, 107, 108, 109, 110, 111, 113, 115, 116, 118,
     120, 122, 124, 126, 129, 131, 134, 136, 139, 142, 144, 147, 150, 153, 156},
    {108, 108, 108, 109, 109, 110, 111, 112, 113, 114, 115, 117, 118, 120, 122,
     124, 126, 128, 130, 132, 134, 137, 139, 142, 144, 147, 150, 153, 156, 158},
    {112, 112, 112, 113, 113, 114, 115, 115, 116, 118, 119, 120, 122, 123, 125,
     127, 129, 131, 133, 135, 138, 140, 142, 145, 148, 150, 153, 156, 158, 161},
    {116, 116, 116, 117, 117, 118, 118, 119, 120, 121, 123, 124, 126, 127, 129,
     131, 132, 134, 137, 139, 141, 143, 146, 148, 151, 153, 156, 158, 161, 164},
};

/* ---- shared ---- */

/* sine_q7 (app/sine.h) for a byte of phase, whole: the plasma looks it up for every cell. */
static const int8_t SIN8[256] = {
    0, 3, 6, 9, 12, 16, 19, 22, 25, 28, 31, 34, 37, 40, 43, 46,
    49, 51, 54, 57, 60, 63, 65, 68, 71, 73, 76, 78, 81, 83, 85, 88,
    90, 92, 94, 96, 98, 100, 102, 104, 106, 107, 109, 111, 112, 113, 115, 116,
    117, 118, 120, 121, 122, 122, 123, 124, 125, 125, 126, 126, 126, 127, 127, 127,
    127, 127, 127, 127, 126, 126, 126, 125, 125, 124, 123, 122, 122, 121, 120, 118,
    117, 116, 115, 113, 112, 111, 109, 107, 106, 104, 102, 100, 98, 96, 94, 92,
    90, 88, 85, 83, 81, 78, 76, 73, 71, 68, 65, 63, 60, 57, 54, 51,
    49, 46, 43, 40, 37, 34, 31, 28, 25, 22, 19, 16, 12, 9, 6, 3,
    0, -3, -6, -9, -12, -16, -19, -22, -25, -28, -31, -34, -37, -40, -43, -46,
    -49, -51, -54, -57, -60, -63, -65, -68, -71, -73, -76, -78, -81, -83, -85, -88,
    -90, -92, -94, -96, -98, -100, -102, -104, -106, -107, -109, -111, -112, -113, -115, -116,
    -117, -118, -120, -121, -122, -122, -123, -124, -125, -125, -126, -126, -126, -127, -127, -127,
    -127, -127, -127, -127, -126, -126, -126, -125, -125, -124, -123, -122, -122, -121, -120, -118,
    -117, -116, -115, -113, -112, -111, -109, -107, -106, -104, -102, -100, -98, -96, -94, -92,
    -90, -88, -85, -83, -81, -78, -76, -73, -71, -68, -65, -63, -60, -57, -54, -51,
    -49, -46, -43, -40, -37, -34, -31, -28, -25, -22, -19, -16, -12, -9, -6, -3,
};

static int32_t sin8(uint32_t phase) {
    return SIN8[phase & 255u];
}

uint32_t scene_random(uint32_t* state) {
    uint32_t x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    return x;
}

uint8_t scene_dark(uint8_t color) {
    static const uint8_t DARK[16] = {
        GRID_BLACK,     GRID_LIGHT_GREY, GRID_BROWN,     GRID_LIGHT_BLUE,
        GRID_BLUE,      GRID_DARK_GREY,  GRID_DARK_GREY, GRID_ORANGE,
        GRID_BROWN,     GRID_DARK_GREY,  GRID_RED,       GRID_DARK_GREY,
        GRID_DARK_GREY, GRID_GREEN,      GRID_BLUE,      GRID_GREY,
    };
    return DARK[color & 15u];
}

/* A voice's colour as RULE30 and the ant draw it: darker under a low cutoff. */
static uint8_t mood(const scene_music* m, uint32_t v) {
    return m->cutoff < SCENE_DARK_CUTOFF ? scene_dark(m->color[v]) : m->color[v];
}

static void put(scene* s, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    s->cv.ch[row][col] = ch;
    s->cv.attr[row][col] = attr;
}

static void blank_rows(scene* s, uint32_t from, uint32_t to) {
    for (uint32_t r = from; r < to; r++) {
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            put(s, c, r, BLANK_CH, BLANK_ATTR);
        }
    }
}

static int32_t clamp(int32_t x, int32_t lo, int32_t hi) {
    return x < lo ? lo : x > hi ? hi : x;
}

/* ---- FIRE ---- */

#define FIRE_TOP (GRID_ROWS - SCENE_FIRE_ROWS)
/* Columns a flame, a voice each. */
#define FIRE_W (GRID_COLS / SCENE_VOICES)
/* The ember bed: the bottom heat row glows at least this (brown, 38..45). Above it, heat at or
 * below FIRE_DARK goes out: in silence the bed holds still, and a flame's tail ends dark. */
#define FIRE_EMBER 38u
#define FIRE_DARK 48u

static const uint8_t FIRE_COLORS[7] = {GRID_BLACK,     GRID_BROWN,  GRID_RED,  GRID_ORANGE,
                                       GRID_LIGHT_RED, GRID_YELLOW, GRID_WHITE};

/* Cell row k of the fire: the upper half the heat row 2k, the lower half 2k + 1. */
static void fire_row(scene* s, uint32_t k) {
    const scene_fire* f = &s->u.fire;
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint8_t top = FIRE_COLORS[f->heat[2u * k][c] * 7u >> 8];
        uint8_t bottom = FIRE_COLORS[f->heat[2u * k + 1u][c] * 7u >> 8];
        put(s, c, FIRE_TOP + k, FONT_Q3, GRID_ATTR(top, bottom));
    }
}

/* One step of the fire: the fuel follows the envelopes, the bottom row burns it, and every row
 * takes the heat of the row below, cooled a little and blown a column either way within its
 * flame (so the flames rise a row a step). */
static void fire_burn(scene* s, const scene_music* m) {
    scene_fire* f = &s->u.fire;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint32_t e = m->env[v] + m->env[v] / 2u; /* a sustain of 10 (170) burns full */
        e = e > 255u ? 255u : e;
        uint32_t hot = f->hot[v];
        if (e) {
            hot -= hot / 16u;
            hot = e > hot ? e : hot;
        } else {
            hot -= (hot + 3u) / 4u; /* a silent voice's flame dies */
        }
        f->hot[v] = (uint8_t)hot;
    }
    /* The cutoff: how hot it burns, from orange (a closed filter) to white. */
    uint32_t scale = 128u + (uint32_t)m->cutoff * 128u / 2047u;
    uint8_t* bed = f->heat[SCENE_FIRE_H - 1u];
    for (uint32_t x = 0; x < GRID_COLS; x++) {
        uint32_t v = x / FIRE_W;
        uint32_t r = scene_random(&s->rng);
        uint32_t heat = f->flare[v] ? 255u : f->hot[v] * (192u + (r >> 4 & 63u)) >> 8;
        heat = heat * scale >> 8;
        uint32_t ember = FIRE_EMBER + (r & 7u);
        bed[x] = (uint8_t)(heat > ember ? heat : ember);
    }
    /* The pitch: higher notes cool slower and burn higher; the resonance lowers the cooling. */
    uint32_t cool[SCENE_VOICES];
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint32_t t =
            m->note[v] == SCENE_NO_NOTE ? 0u : (uint32_t)clamp((int32_t)m->note[v] - 36, 0, 48);
        int32_t c = 13 - (int32_t)(t * 6u / 48u) - (int32_t)(m->resonance / 4u);
        cool[v] = (uint32_t)(c < 2 ? 2 : c) + 1u;
        f->flare[v] = 0;
    }
    for (uint32_t y = 0; y + 1u < SCENE_FIRE_H; y++) {
        const uint8_t* below = f->heat[y + 1u];
        uint8_t* row = f->heat[y];
        for (uint32_t x = 0; x < GRID_COLS; x++) {
            uint32_t v = x / FIRE_W;
            uint32_t r = scene_random(&s->rng);
            int32_t dx = (int32_t)x + (int32_t)((r & 255u) * 3u >> 8) - 1;
            uint32_t to =
                (uint32_t)clamp(dx, (int32_t)(v * FIRE_W), (int32_t)(v * FIRE_W + FIRE_W - 1u));
            uint32_t c = (r >> 8 & 255u) * cool[v] >> 8;
            row[to] = (uint8_t)(below[x] > c + FIRE_DARK ? below[x] - c : 0u);
        }
    }
}

static void fire_start(scene* s) {
    blank_rows(s, 0, FIRE_TOP);
    for (uint32_t k = 0; k < SCENE_FIRE_ROWS; k++) {
        fire_row(s, k);
    }
}

/* It burns every other frame, two steps at once so the flames rise as fast as at 25 fps, and
 * draws half its rows a frame, the even ones on the frame it burns and the odd ones on the
 * next: 12.5 fps, half the area a frame. */
static void fire_step(scene* s, const scene_music* m) {
    scene_fire* f = &s->u.fire;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        f->flare[v] = (uint8_t)(f->flare[v] | (m->trig[v] != 0)); /* kept for the next burn */
    }
    uint32_t odd = s->frame & 1u;
    if (!odd) {
        fire_burn(s, m);
        fire_burn(s, m);
    }
    for (uint32_t k = odd; k < SCENE_FIRE_ROWS; k += 2u) {
        fire_row(s, k);
    }
}

/* ---- PLASMA ---- */

#define PLASMA_MID 15
/* The sines' spatial steps, 65536 a turn: across 0.28 rad a cell, down 0.22, round the centre
 * 0.35 (a quarter cell of scene_dist4: 0.0875), the diagonal 0.10 + 0.25 resonance / 15. */
#define PLASMA_X 2921u
#define PLASMA_Y 2294u
#define PLASMA_R 913u
#define PLASMA_D 1043u
#define PLASMA_D_RES 174u
/* A ripple: a bright ring RING_W quarter cells wide each side, growing RING_SPEED quarter cells
 * a frame (half a cell) from its note-on; its strength at the note-on (0.22 of the plasma's
 * range) falls exp(-3 t), 0.887 a frame. One ring, not a train of them: a frame changes the
 * cells on its edge, not every cell round it. */
#define RING_W 8u
#define RING_SPEED 2u
#define RIPPLE_AMP 224u
#define RIPPLE_FALL 227u
/* Voice 3's warmth: up to 0.18 of the range, moving at most WARM_STEP a frame toward its
 * envelope, so a drum's envelope does not move every cell at once. */
#define WARM_MAX 183u
#define WARM_STEP 4u
/* The four sums' range is -508..508; a level is one of 20 (5 ramp steps of 4 dithers). */
#define PLASMA_SPAN 1016

static const uint8_t PLASMA_RAMP[6] = {GRID_BLACK,      GRID_BLUE, GRID_PURPLE,
                                       GRID_LIGHT_BLUE, GRID_CYAN, GRID_WHITE};

/* Cell row y of the plasma at its phases now. */
static void plasma_row(scene* s, uint32_t y, const int32_t* across, const int32_t* diag,
                       const scene_ripple* rip, uint32_t ripples) {
    const scene_plasma* p = &s->u.plasma;
    int32_t down = sin8((y * PLASMA_Y - p->phase[1]) >> 8);
    int32_t boost = p->warm; /* voice 3 warms it */
    uint32_t dy = (uint32_t)(y < PLASMA_MID ? PLASMA_MID - y : y - PLASMA_MID);
    for (uint32_t x = 0; x < GRID_COLS; x++) {
        uint32_t dx = (uint32_t)(x < PLASMA_MID ? PLASMA_MID - x : x - PLASMA_MID);
        int32_t v = across[x] + down + diag[x + y] +
                    sin8((scene_dist4[dy][dx] * PLASMA_R - p->phase[3]) >> 8);
        v += PLASMA_SPAN / 2 + boost;
        for (uint32_t k = 0; k < ripples; k++) {
            const scene_ripple* r = &rip[k];
            uint32_t d = scene_dist4[y < r->y ? r->y - y : y - r->y][x < r->x ? r->x - x : x - r->x];
            uint32_t rad = r->age * RING_SPEED;
            uint32_t off = d < rad ? rad - d : d - rad;
            if (off < RING_W) {
                v += (int32_t)(r->amp * (RING_W - off) / RING_W);
            }
        }
        uint32_t level = (uint32_t)clamp(v, 0, PLASMA_SPAN) * 1289u >> 16; /* * 20 / 1017 */
        uint32_t pr = level / 4u;
        put(s, x, y, (uint8_t)(FONT_D0 + 4u * (level % 4u)),
            GRID_ATTR(PLASMA_RAMP[pr + 1u], PLASMA_RAMP[pr]));
    }
}

/* Rows from, from + by, .. below to. */
static void plasma_rows(scene* s, const scene_music* m, uint32_t from, uint32_t to,
                        uint32_t by) {
    const scene_plasma* p = &s->u.plasma;
    int32_t across[GRID_COLS];
    int32_t diag[2u * GRID_COLS - 1u];
    uint32_t twist = PLASMA_D + PLASMA_D_RES * m->resonance;
    for (uint32_t x = 0; x < GRID_COLS; x++) {
        across[x] = sin8((x * PLASMA_X + p->phase[0]) >> 8);
    }
    for (uint32_t k = 0; k < 2u * GRID_COLS - 1u; k++) {
        diag[k] = sin8((k * twist + p->phase[2]) >> 8);
    }
    scene_ripple rip[SCENE_VOICES]; /* the ripples running */
    uint32_t ripples = 0;
    for (uint32_t k = 0; k < SCENE_VOICES; k++) {
        if (p->rip[k].age < SCENE_RIPPLE_FRAMES) {
            rip[ripples++] = p->rip[k];
        }
    }
    for (uint32_t y = from; y < to; y += by) {
        plasma_row(s, y, across, diag, rip, ripples);
    }
}

/* No rows: the start is drawn as scene_fill asks for it. */
static void plasma_start(scene* s) {
    for (uint32_t k = 0; k < SCENE_VOICES; k++) {
        s->u.plasma.rip[k].age = SCENE_RIPPLE_FRAMES;
    }
}

/* The cutoff sets the speed (0.1 to 0.8 rad a second, on its square: the prototype's 3.7 at
 * the top changed more cells a frame than the budget has), the envelopes how much of it runs
 * (an eighth in silence), and each sine runs at its own multiple of it (1, 0.7 back, 1.3 and
 * 1.7 back). It draws every other row a frame. */
static void plasma_step(scene* s, const scene_music* m) {
    scene_plasma* p = &s->u.plasma;
    uint32_t loud = 0;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        loud = m->env[v] > loud ? m->env[v] : loud;
    }
    uint32_t c = m->cutoff;
    uint32_t speed = (40u + c * c / 14000u) * (32u + loud * 7u / 8u) >> 8;
    p->phase[0] += speed;
    p->phase[1] += speed * 7u / 10u;
    p->phase[2] += speed * 13u / 10u;
    p->phase[3] += speed * 17u / 10u;
    uint32_t warm = m->env[2] * WARM_MAX >> 8;
    p->warm = (uint8_t)(warm > p->warm + WARM_STEP   ? p->warm + WARM_STEP
                        : warm + WARM_STEP < p->warm ? p->warm - WARM_STEP
                                                     : warm);
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        scene_ripple* r = &p->rip[v];
        if (m->trig[v] && m->note[v] != SCENE_NO_NOTE) {
            /* The pitch class across, the pitch up. */
            r->x = (uint8_t)(PLASMA_MID + ((int32_t)(m->note[v] % 12u) - 6) * 2);
            r->y = (uint8_t)clamp(PLASMA_MID - ((int32_t)m->note[v] - 72) / 2, 0,
                                  (int32_t)GRID_ROWS - 1);
            r->age = 0;
            r->amp = RIPPLE_AMP;
        }
        if (r->age < SCENE_RIPPLE_FRAMES) {
            r->age++;
            r->amp = (uint8_t)(r->amp * RIPPLE_FALL >> 8);
        }
    }
    plasma_rows(s, m, s->frame & 1u, GRID_ROWS, 2u);
}

/* ---- RULE 30 ---- */

#define R30_W SCENE_RULE30_W
#define R30_H SCENE_RULE30_H
#define R30_MASK ((1ull << R30_W) - 1u)

uint64_t scene_rule30_next(uint64_t cur) {
    cur &= R30_MASK;
    uint64_t left = ((cur << 1) | (cur >> (R30_W - 1u))) & R30_MASK; /* bit i: cell i - 1 */
    uint64_t right = (cur >> 1) | ((cur & 1u) << (R30_W - 1u));      /* bit i: cell i + 1 */
    return left ^ (cur | right);
}

/* A generation on the head's row in color, the two rows ahead cleared; dirty gets the cell
 * rows it touched. */
static void r30_generation(scene* s, uint8_t color, uint32_t* dirty) {
    scene_rule30* t = &s->u.rule30;
    uint32_t h = t->head;
    t->cur = scene_rule30_next(t->cur);
    t->row[h] = t->cur;
    t->color[h] = color;
    *dirty |= 1u << (h / 2u);
    for (uint32_t k = 1; k <= 2u; k++) {
        uint32_t a = (h + k) % R30_H;
        t->row[a] = 0;
        *dirty |= 1u << (a / 2u);
    }
    t->head = (uint8_t)((h + 1u) % R30_H);
}

/* Cell row r: rows 2r and 2r + 1 as quadrants, in the lower row's colour where it has a pixel. */
static void r30_draw(scene* s, uint32_t r) {
    const scene_rule30* t = &s->u.rule30;
    uint64_t a = t->row[2u * r];
    uint64_t b = t->row[2u * r + 1u];
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint32_t q = (uint32_t)(a >> (2u * c) & 3u) | (uint32_t)(b >> (2u * c) & 3u) << 2;
        if (!q) {
            put(s, c, r, BLANK_CH, BLANK_ATTR);
        } else {
            uint8_t fg = q & 12u ? t->color[2u * r + 1u] : t->color[2u * r];
            put(s, c, r, (uint8_t)(FONT_Q0 + q), GRID_ATTR(fg, GRID_BLACK));
        }
    }
}

static void r30_start(scene* s) {
    scene_rule30* t = &s->u.rule30;
    uint32_t dirty = 0;
    t->cur = 1ull << (R30_W / 2u);
    for (uint32_t g = 0; g < SCENE_RULE30_START; g++) {
        r30_generation(s, GRID_GREY, &dirty);
    }
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        r30_draw(s, r);
    }
}

/* A note-on steps it in its voice's colour (voice 3's flips the cell its pitch picks first);
 * the loudest held voice keeps it stepping with its envelope. */
static void r30_step(scene* s, const scene_music* m) {
    scene_rule30* t = &s->u.rule30;
    uint32_t dirty = 0;
    uint32_t steps = 0;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        if (!m->trig[v] || steps >= SCENE_RULE30_MAX_STEPS) {
            continue;
        }
        if (v == 2u && m->note[v] != SCENE_NO_NOTE) {
            t->cur ^= 1ull << ((m->note[v] * 7u + s->frame * 11u) % R30_W);
        }
        r30_generation(s, mood(m, v), &dirty);
        steps++;
    }
    uint32_t loud = SCENE_VOICES;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        if (m->gate[v] && m->env[v] && (loud == SCENE_VOICES || m->env[v] > m->env[loud])) {
            loud = v;
        }
    }
    if (loud == SCENE_VOICES) {
        t->acc = 0;
    } else {
        t->acc = (uint16_t)(t->acc + m->env[loud]);
        if (t->acc >= SCENE_ENV_STEP) {
            t->acc = (uint16_t)(t->acc - SCENE_ENV_STEP);
            if (steps < SCENE_RULE30_MAX_STEPS) {
                r30_generation(s, mood(m, loud), &dirty);
            }
        }
    }
    for (uint32_t r = 0; dirty >> r; r++) {
        if (dirty >> r & 1u) {
            r30_draw(s, r);
        }
    }
}

/* ---- STARS ---- */

#define STAR_NEAR 205
#define STAR_FAR 4096
#define STAR_BOX 768

static const uint8_t STAR_GLYPH[4] = {FONT_TINY, FONT_DOT, FONT_PLUS, FONT_STAR};
static const uint8_t STAR_COLOR[4] = {GRID_GREY, GRID_LIGHT_GREY, GRID_WHITE, GRID_WHITE};

/* Far to near: 0 past 0.75 of the depth, 1 past 0.45, 2 past 0.22, 3 the nearest. */
static uint32_t star_layer(uint32_t z) {
    return z > 3072u ? 0u : z > 1843u ? 1u : z > 901u ? 2u : 3u;
}

static void star_place(scene* s, scene_star* p) {
    p->x = (int16_t)(scene_random(&s->rng) % (2u * STAR_BOX + 1u)) - STAR_BOX;
    p->y = (int16_t)(scene_random(&s->rng) % (2u * STAR_BOX + 1u)) - STAR_BOX;
}

static void stars_draw(scene* s, const scene_music* m) {
    const scene_stars* t = &s->u.stars;
    blank_rows(s, 0, GRID_ROWS);
    for (uint32_t layer = 0; layer < 4u; layer++) {
        for (uint32_t i = 0; i < SCENE_STARS_N; i++) {
            const scene_star* p = &t->st[i];
            if (star_layer(p->z) != layer) {
                continue;
            }
            int32_t sx = 120 + t->cx + (int32_t)p->x * 640 / (int32_t)p->z;
            int32_t sy = 120 + (int32_t)p->y * 640 / (int32_t)p->z;
            if (sx < 0 || sx >= 240 || sy < 0 || sy >= 240) {
                continue;
            }
            uint32_t v = i % SCENE_VOICES;
            uint8_t fg = m && t->lit[v] ? m->color[v] : STAR_COLOR[layer];
            put(s, (uint32_t)sx / 8u, (uint32_t)sy / 8u, STAR_GLYPH[layer],
                GRID_ATTR(fg, GRID_BLACK));
        }
    }
}

static void stars_start(scene* s) {
    scene_stars* t = &s->u.stars;
    for (uint32_t i = 0; i < SCENE_STARS_N; i++) {
        star_place(s, &t->st[i]);
        t->st[i].z = (uint16_t)(STAR_NEAR + scene_random(&s->rng) % (STAR_FAR - STAR_NEAR + 1u));
    }
    stars_draw(s, NULL);
}

/* They come closer at the cruising speed (the cutoff's), a voice's third faster with its
 * envelope; a note-on lights its voice's stars and steers the centre toward its pitch. */
static void stars_step(scene* s, const scene_music* m) {
    scene_stars* t = &s->u.stars;
    int32_t cruise = 8 + (int32_t)m->cutoff * 24 / 2047;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        if (m->trig[v]) {
            t->lit[v] = SCENE_STARS_LIT;
            if (m->note[v] != SCENE_NO_NOTE) {
                t->aim = (int16_t)clamp(((int32_t)m->note[v] - 60) * 4, -80, 80);
            }
        } else if (t->lit[v]) {
            t->lit[v]--;
        }
        if (!m->env[v]) {
            t->lit[v] = 0;
        }
    }
    int32_t d = t->aim - t->cx;
    int32_t move = d / 8;
    t->cx = (int16_t)(t->cx + (move ? move : d > 0 ? 1 : d < 0 ? -1 : 0));
    for (uint32_t i = 0; i < SCENE_STARS_N; i++) {
        scene_star* p = &t->st[i];
        uint32_t v = i % SCENE_VOICES;
        int32_t z = (int32_t)p->z - cruise - (m->env[v] * 180 >> 8);
        if (z <= STAR_NEAR) {
            star_place(s, p);
            z = STAR_FAR;
        }
        p->z = (uint16_t)z;
    }
    stars_draw(s, m);
}

/* ---- ANT ---- */

static const int8_t ANT_DX[4] = {0, 1, 0, -1};
static const int8_t ANT_DY[4] = {-1, 0, 1, 0};
static const scene_ant_walker ANT_HOME[SCENE_VOICES] = {{9, 12, 0}, {20, 12, 1}, {15, 20, 2}};

static void ant_cell(scene* s, uint32_t x, uint32_t y) {
    uint8_t k = s->u.ant.cell[y][x];
    if (k) {
        put(s, x, y, FONT_PIP, GRID_ATTR(k - 1u, GRID_BLACK));
    } else {
        put(s, x, y, BLANK_CH, BLANK_ATTR);
    }
}

static void ant_balls(scene* s, const scene_music* m) {
    const scene_ant* t = &s->u.ant;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint8_t fg = m && m->env[v] ? GRID_WHITE : GRID_DARK_GREY;
        put(s, t->a[v].x, t->a[v].y, FONT_BALL, GRID_ATTR(fg, GRID_BLACK));
    }
}

static void ant_start(scene* s) {
    scene_ant* t = &s->u.ant;
    memset(t, 0, sizeof *t);
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        t->a[v] = ANT_HOME[v];
    }
    blank_rows(s, 0, GRID_ROWS);
    ant_balls(s, NULL);
}

/* Langton's rule: on an empty cell turn right and paint it, on a painted one turn left and
 * clear it; then a step forward, round the edges. */
static void ant_walk(scene* s, uint32_t v, uint8_t color) {
    scene_ant* t = &s->u.ant;
    scene_ant_walker* a = &t->a[v];
    uint8_t* k = &t->cell[a->y][a->x];
    if (*k == 0) {
        a->d = (uint8_t)((a->d + 1u) & 3u);
        *k = (uint8_t)(color + 1u);
    } else {
        a->d = (uint8_t)((a->d + 3u) & 3u);
        *k = 0;
    }
    ant_cell(s, a->x, a->y);
    a->x = (uint8_t)((a->x + GRID_COLS + (uint32_t)(int32_t)ANT_DX[a->d]) % GRID_COLS);
    a->y = (uint8_t)((a->y + GRID_ROWS + (uint32_t)(int32_t)ANT_DY[a->d]) % GRID_ROWS);
}

/* A note-on walks its ant SCENE_ANT_STEPS steps, a sounding voice a step every SCENE_ENV_STEP
 * of its envelope; the canvas changes only where an ant went. */
static void ant_step(scene* s, const scene_music* m) {
    scene_ant* t = &s->u.ant;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        ant_cell(s, t->a[v].x, t->a[v].y); /* the ball goes */
    }
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint32_t n = m->trig[v] ? SCENE_ANT_STEPS : 0u;
        if (m->env[v]) {
            uint32_t acc = t->acc[v] + m->env[v];
            for (; acc >= SCENE_ENV_STEP; acc -= SCENE_ENV_STEP) {
                n++;
            }
            t->acc[v] = (uint16_t)acc;
        } else {
            t->acc[v] = 0;
        }
        for (; n > 0; n--) {
            ant_walk(s, v, mood(m, v));
            if (++t->steps >= SCENE_ANT_RESET) {
                ant_start(s); /* a clean board */
                break;
            }
        }
    }
    ant_balls(s, m);
}

/* ---- the pieces ---- */

static const scene_music REST = {
    {0}, {0}, {SCENE_NO_NOTE, SCENE_NO_NOTE, SCENE_NO_NOTE}, {0}, {0}, 0, 0};

void scene_begin(scene* s, uint32_t piece) {
    memset(&s->u, 0, sizeof s->u);
    s->piece = (uint8_t)(piece % SCENE_PIECES);
    s->frame = 0;
    s->rng = 0x9E3779B9u * (s->piece + 1u);
    s->filled = GRID_ROWS;
    switch (s->piece) {
    case SCENE_FIRE: fire_start(s); break;
    case SCENE_PLASMA:
        plasma_start(s);
        s->filled = 0;
        break;
    case SCENE_RULE30: r30_start(s); break;
    case SCENE_STARS: stars_start(s); break;
    default: ant_start(s); break;
    }
}

void scene_fill(scene* s, const scene_music* m, uint32_t rows) {
    rows = rows < GRID_ROWS ? rows : GRID_ROWS;
    if (s->filled >= rows) {
        return;
    }
    plasma_rows(s, m ? m : &REST, s->filled, rows, 1u); /* the only piece that fills */
    s->filled = (uint8_t)rows;
}

void scene_start(scene* s, uint32_t piece, const scene_music* m) {
    scene_begin(s, piece);
    scene_fill(s, m, GRID_ROWS);
}

void scene_step(scene* s, const scene_music* m) {
    scene_fill(s, m, GRID_ROWS);
    switch (s->piece) {
    case SCENE_FIRE: fire_step(s, m); break;
    case SCENE_PLASMA: plasma_step(s, m); break;
    case SCENE_RULE30: r30_step(s, m); break;
    case SCENE_STARS: stars_step(s, m); break;
    default: ant_step(s, m); break;
    }
    s->frame++;
}
