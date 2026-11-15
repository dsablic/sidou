// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "art_pack.h"

#include <stddef.h>

_Static_assert(SCENE_ART_PAGE == STORE_CHUNK, "a pack's page is the store's chunk");
_Static_assert(SCENE_ART_PAGES * SCENE_ART_PAGE >= STORE_FILE_MAX, "pages for the largest file");
_Static_assert(ART_PACK_FRAMES_END <= SCENE_ART_PAGE, "the tables are on the first page");

static const uint8_t MAGIC[8] = {'S', 'I', 'D', 'O', 'U', 'A', 'R', 'T'};

const art_pack_slot art_pack_rmr_slots[ART_PACK_RMR_SLOTS] = {
    {1, 64, 124, 2, SCENE_RMR_POSES},
    {2, 64, 124, 2, SCENE_RMR_POSES},
    {3, 64, 124, 2, SCENE_RMR_UNARMED_POSES},
    {4, 64, 124, 2, SCENE_RMR_UNARMED_POSES},
    {5, 42, 84, 2, SCENE_RMR_SWORD_FRAMES},
    {6, 16, 44, 2, 1},
    {7, 240, 124, 1, SCENE_RMR_SKIES},
    {8, 240, 240, 1, 1},
    {9, 240, 16, 1, 1},
    {10, 240, 16, 1, 1},
    {11, 240, 16, 1, 1},
    {12, 6, 5, 1, 1},
};

static uint32_t rd16(const uint8_t* p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8;
}

static uint32_t rd32(const uint8_t* p) {
    return rd16(p) | rd16(p + 2) << 16;
}

static int name_ok(const uint8_t* p) {
    uint32_t n = 0;
    while (n < STORE_NAME_MAX && p[n]) {
        if (p[n] < 0x20u || p[n] > 0x5Fu) {
            return 0;
        }
        n++;
    }
    for (uint32_t i = n; i < STORE_NAME_MAX; i++) {
        if (p[i]) {
            return 0;
        }
    }
    return n && p[0] != ' ';
}

int art_pack_head(const uint8_t* p, uint32_t n, uint32_t length) {
    if (length < ART_PACK_FRAMES_END + ART_PACK_CRC || length > STORE_FILE_MAX ||
        n < ART_PACK_TABLES) {
        return ART_PACK_SHORT;
    }
    for (uint32_t i = 0; i < sizeof MAGIC; i++) {
        if (p[i] != MAGIC[i]) {
            return ART_PACK_MAGIC;
        }
    }
    if (p[8] != ART_PACK_VERSION) {
        return ART_PACK_NEWER;
    }
    if (p[9] != ART_PACK_SCENE_RMR) {
        return ART_PACK_SCENE;
    }
    if (p[10] != ART_PACK_RMR_SLOTS || p[11] != SCENE_RMR_ART_FRAMES || rd32(p + 12) != length) {
        return ART_PACK_SLOTS;
    }
    if (!name_ok(p + 16)) {
        return ART_PACK_NAME;
    }
    const uint8_t* e = p + ART_PACK_HEAD;
    for (uint32_t i = 0; i < ART_PACK_RMR_SLOTS; i++, e += ART_PACK_SLOT) {
        const art_pack_slot* s = &art_pack_rmr_slots[i];
        if (e[0] != s->id || e[1] != s->w || e[2] != s->h || e[3] != s->frames) {
            return ART_PACK_SLOTS;
        }
    }
    return ART_PACK_OK;
}

static int page_span(uint32_t off, uint32_t n) {
    return off / SCENE_ART_PAGE == (off + n - 1u) / SCENE_ART_PAGE;
}

static int frame_load(scene_rmr_art* a, uint32_t i, const art_pack_slot* s, const uint8_t* e,
                      uint32_t data_end) {
    scene_image* f = &a->frame[i];
    uint32_t base = e[8], rows = rd16(e + 10);
    if (!e[2] || !e[3] || (uint32_t)e[0] + e[2] > s->w || (uint32_t)e[1] + e[3] > s->h ||
        e[4] != s->gx || e[5] > 15u || e[9]) {
        return ART_PACK_FRAME_BAD;
    }
    if (base != ART_PACK_NO_BASE &&
        (base >= i || a->frame[base].fw != s->w || a->frame[base].fh != s->h ||
         a->frame[base].gx != s->gx)) {
        return ART_PACK_FRAME_BAD;
    }
    uint32_t bytes = 2u * e[3];
    if ((rows & 1u) || rows < ART_PACK_FRAMES_END || rows + bytes > data_end ||
        !page_span(rows, bytes)) {
        return ART_PACK_ROWS;
    }
    const uint16_t* r = (const uint16_t*)(const void*)(a->page[rows / SCENE_ART_PAGE] +
                                                       rows % SCENE_ART_PAGE);
    for (uint32_t y = 0; y < e[3]; y++) {
        if (r[y] < ART_PACK_FRAMES_END || r[y] >= data_end) {
            return ART_PACK_ROWS;
        }
    }
    f->fw = s->w, f->fh = s->h;
    f->x = e[0], f->y = e[1], f->w = e[2], f->h = e[3];
    f->gx = e[4], f->clear = e[5], f->ax = e[6], f->ay = e[7];
    f->rows = r;
    f->page = a->page;
    f->base = base == ART_PACK_NO_BASE ? NULL : &a->frame[base];
    return ART_PACK_OK;
}

int art_pack_load(scene_rmr_art* a, uint32_t length, uint32_t crc) {
    const uint8_t* p = a->page[0];
    int why = art_pack_head(p, length < SCENE_ART_PAGE ? length : SCENE_ART_PAGE, length);
    if (why) {
        return why;
    }
    if (crc != ART_PACK_RESIDUE) {
        return ART_PACK_CRC_BAD;
    }
    const uint8_t* e = p + ART_PACK_TABLES;
    uint32_t i = 0;
    for (uint32_t k = 0; k < ART_PACK_RMR_SLOTS; k++) {
        const art_pack_slot* s = &art_pack_rmr_slots[k];
        for (uint32_t n = 0; n < s->frames; n++, i++, e += ART_PACK_FRAME) {
            why = frame_load(a, i, s, e, length - ART_PACK_CRC);
            if (why) {
                return why;
            }
        }
    }
    return ART_PACK_OK;
}

int art_pack_open(scene_rmr_art* a, const store* st, store_file* f) {
    if (!st || store_file_open(st, ART_PACK_RMR_NAME, f) != STORE_OK || f->type != STORE_FILE_ART) {
        return ART_PACK_NONE;
    }
    for (uint32_t i = 0; i < f->chunks; i++) {
        a->page[i] = store_file_page(st, f, i);
    }
    return art_pack_load(a, f->length, f->crc);
}

int art_pack_bytes(scene_rmr_art* a, const uint8_t* p, uint32_t length) {
    if (length > STORE_FILE_MAX) {
        return ART_PACK_SHORT;
    }
    for (uint32_t i = 0; i * SCENE_ART_PAGE < length; i++) {
        a->page[i] = p + i * SCENE_ART_PAGE;
    }
    return art_pack_load(a, length, store_crc32(0, p, length));
}
