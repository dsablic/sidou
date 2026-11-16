// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "scene.h"

#include <string.h>

#include "font.h"

#define BLANK_CH ((uint8_t)' ')
#define BLANK_ATTR GRID_ATTR(GRID_BLACK, GRID_BLACK)

const char* const scene_names[SCENE_PIECES] = {"FIRE", "PLASMA", "SCOPE", "DEMO",
                                               "RED MOON RITE"};

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

const int8_t scene_sin8[256] = {
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
    return scene_sin8[phase & 255u];
}

uint32_t scene_random(uint32_t* state) {
    uint32_t x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    return x;
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

#define FIRE_TOP (GRID_ROWS - SCENE_FIRE_ROWS)
#define FIRE_W (GRID_COLS / SCENE_VOICES)
#define FIRE_EMBER 38u
#define FIRE_IDLE_HOT 120u
#define FIRE_DARK 48u

#define FIRE_LIGHT 135u
#define FIRE_YELLOW 16u
#define FIRE_WHITE_OPEN 196u
#define FIRE_WHITE_SHUT 300u

static uint8_t fire_color(uint32_t heat, uint32_t white_at) {
    return heat >= white_at                ? GRID_WHITE
           : heat + FIRE_YELLOW >= white_at ? GRID_YELLOW
           : heat >= FIRE_LIGHT             ? GRID_LIGHT_RED
           : heat > FIRE_DARK               ? GRID_RED
                                            : GRID_BLACK;
}

static void fire_row(scene* s, uint32_t k, uint32_t white_at) {
    const scene_fire* f = &s->u.fire;
    const uint8_t* up = f->heat[2u * k];
    const uint8_t* low = f->heat[2u * k + 1u];
    int bed = 2u * k + 1u == SCENE_FIRE_H - 1u;
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint8_t top = fire_color(up[c], white_at);
        uint8_t bottom = bed && low[c] && low[c] <= FIRE_DARK ? GRID_BROWN
                                                               : fire_color(low[c], white_at);
        put(s, c, FIRE_TOP + k, FONT_Q3, GRID_ATTR(top, bottom));
    }
}

static uint32_t fire_white(const scene_music* m) {
    uint32_t c = m->cutoff;
    uint32_t k = c * (4094u - c) / 2047u;
    return FIRE_WHITE_SHUT - k * (FIRE_WHITE_SHUT - FIRE_WHITE_OPEN) / 2047u;
}

static void fire_burn(scene* s, const scene_music* m) {
    scene_fire* f = &s->u.fire;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint32_t e = m->env[v] + m->env[v] / 2u;
        e = e > 255u ? 255u : e;
        uint32_t hot = f->hot[v];
        if (e) {
            hot -= hot / 16u;
            hot = e > hot ? e : hot;
        } else {
            hot -= (hot + 3u) / 4u;
            hot = hot < FIRE_IDLE_HOT ? FIRE_IDLE_HOT : hot;
        }
        f->hot[v] = (uint8_t)hot;
    }
    uint8_t* bed = f->heat[SCENE_FIRE_H - 1u];
    for (uint32_t x = 0; x < GRID_COLS; x++) {
        uint32_t v = x / FIRE_W;
        uint32_t r = scene_random(&s->rng);
        int32_t sway = sin8(x * 23u + f->burns * 5u) + sin8(x * 61u - f->burns * 7u);
        uint32_t fuel = (uint32_t)(184 + (sway * 88 >> 8)) + (r >> 4 & 31u);
        fuel = fuel > 255u ? 255u : fuel;
        uint32_t heat = f->flare[v] ? 255u : f->hot[v] * fuel >> 8;
        uint32_t ember = FIRE_EMBER + (r & 7u);
        bed[x] = (uint8_t)(heat > ember ? heat : ember);
    }
    f->burns++;
    uint32_t cool[SCENE_VOICES];
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint32_t t =
            m->note[v] == SCENE_NO_NOTE ? 0u : (uint32_t)clamp((int32_t)m->note[v] - 36, 0, 48);
        int32_t c = 13 - (int32_t)(t * 6u / 48u) - (int32_t)(m->resonance / 4u);
        cool[v] = (uint32_t)(c < 2 ? 2 : c) + 1u;
        f->flare[v] = 0;
    }
    uint32_t rng = s->rng;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        uint32_t lo = v * FIRE_W;
        uint32_t hi = lo + FIRE_W - 1u;
        uint32_t k = cool[v];
        for (uint32_t y = 0; y + 1u < SCENE_FIRE_H; y++) {
            const uint8_t* below = f->heat[y + 1u];
            uint8_t* row = f->heat[y];
            for (uint32_t x = lo; x <= hi; x++) {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                uint32_t r = rng;
                uint32_t to = x;
                if ((r & 3u) == 0u && x > lo) {
                    to--;
                } else if ((r & 3u) == 1u && x < hi) {
                    to++;
                }
                uint32_t c = ((r >> 8 & 255u) * k >> 8) + ((r >> 16 & 7u) ? 0u : 3u * k);
                row[to] = (uint8_t)(below[x] > c + FIRE_DARK ? below[x] - c : 0u);
            }
        }
    }
    s->rng = rng;
}

static void fire_start(scene* s) {
    blank_rows(s, 0, FIRE_TOP);
    for (uint32_t k = 0; k < SCENE_FIRE_ROWS; k++) {
        fire_row(s, k, FIRE_WHITE_SHUT);
    }
}

static void fire_step(scene* s, const scene_music* m) {
    scene_fire* f = &s->u.fire;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        f->flare[v] = (uint8_t)(f->flare[v] | (m->trig[v] != 0));
    }
    uint32_t odd = s->frame & 1u;
    if (!odd) {
        fire_burn(s, m);
        fire_burn(s, m);
    }
    for (uint32_t k = odd; k < SCENE_FIRE_ROWS; k += 2u) {
        fire_row(s, k, fire_white(m));
    }
}

#define PLASMA_MID 15
#define PLASMA_X 2921u
#define PLASMA_Y 2294u
#define PLASMA_R 913u
#define PLASMA_D 1043u
#define PLASMA_D_RES 174u
#define RING_W 8u
#define RING_SPEED 2u
#define RIPPLE_AMP 224u
#define RIPPLE_FALL 227u
#define WARM_MAX 183u
#define WARM_STEP 4u
#define PLASMA_SPAN 1016

static const uint8_t PLASMA_RAMP[6] = {GRID_BLACK,      GRID_BLUE, GRID_PURPLE,
                                       GRID_LIGHT_BLUE, GRID_CYAN, GRID_WHITE};

static void plasma_row(scene* s, uint32_t y, const int32_t* across, const int32_t* diag,
                       const scene_ripple* rip, uint32_t ripples) {
    const scene_plasma* p = &s->u.plasma;
    int32_t down = sin8((y * PLASMA_Y - p->phase[1]) >> 8);
    int32_t boost = p->warm;
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
        uint32_t level = (uint32_t)clamp(v, 0, PLASMA_SPAN) * 1289u >> 16;
        uint32_t pr = level / 4u;
        put(s, x, y, (uint8_t)(FONT_D0 + 4u * (level % 4u)),
            GRID_ATTR(PLASMA_RAMP[pr + 1u], PLASMA_RAMP[pr]));
    }
}

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
    scene_ripple rip[SCENE_VOICES];
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

static void plasma_start(scene* s) {
    for (uint32_t k = 0; k < SCENE_VOICES; k++) {
        s->u.plasma.rip[k].age = SCENE_RIPPLE_FRAMES;
    }
}

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

static const scene_music REST = {
    {0}, {0}, {SCENE_NO_NOTE, SCENE_NO_NOTE, SCENE_NO_NOTE}, {0}, 0, 0};

void scene_begin(scene* s, uint32_t piece) {
    memset(&s->u, 0, sizeof s->u);
    s->piece = (uint8_t)(piece % SCENE_PETSCII);
    s->frame = 0;
    s->rng = 0x9E3779B9u * (s->piece + 1u);
    s->filled = GRID_ROWS;
    if (s->piece == SCENE_PLASMA) {
        plasma_start(s);
        s->filled = 0;
    } else {
        fire_start(s);
    }
}

void scene_fill(scene* s, const scene_music* m, uint32_t rows) {
    rows = rows < GRID_ROWS ? rows : GRID_ROWS;
    if (s->filled >= rows) {
        return;
    }
    plasma_rows(s, m ? m : &REST, s->filled, rows, 1u);
    s->filled = (uint8_t)rows;
}

void scene_start(scene* s, uint32_t piece, const scene_music* m) {
    scene_begin(s, piece);
    scene_fill(s, m, GRID_ROWS);
}

void scene_step(scene* s, const scene_music* m) {
    scene_fill(s, m, GRID_ROWS);
    if (s->piece == SCENE_PLASMA) {
        plasma_step(s, m);
    } else {
        fire_step(s, m);
    }
    s->frame++;
}
