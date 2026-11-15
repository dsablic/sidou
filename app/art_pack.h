// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_ART_PACK_H
#define SIDOU_ART_PACK_H

#include <stdint.h>

#include "scene_art.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

#define ART_PACK_VERSION 1u
#define ART_PACK_HEAD 32u
#define ART_PACK_SLOT 4u
#define ART_PACK_FRAME 12u
#define ART_PACK_CRC 4u
#define ART_PACK_NO_BASE 0xFFu
#define ART_PACK_RESIDUE 0x2144DF1Cu
#define ART_PACK_SCENE_RMR 1u
#define ART_PACK_RMR_NAME "RED MOON RITE"
#define ART_PACK_RMR_SLOTS 12u
#define ART_PACK_TABLES (ART_PACK_HEAD + ART_PACK_RMR_SLOTS * ART_PACK_SLOT)
#define ART_PACK_FRAMES_END (ART_PACK_TABLES + SCENE_RMR_ART_FRAMES * ART_PACK_FRAME)

enum {
    ART_PACK_OK = 0,
    ART_PACK_NONE,
    ART_PACK_SHORT,
    ART_PACK_MAGIC,
    ART_PACK_NEWER,
    ART_PACK_SCENE,
    ART_PACK_NAME,
    ART_PACK_SLOTS,
    ART_PACK_FRAME_BAD,
    ART_PACK_ROWS,
    ART_PACK_CRC_BAD,
};

typedef struct {
    uint8_t id, w, h, gx, frames;
} art_pack_slot;

extern const art_pack_slot art_pack_rmr_slots[ART_PACK_RMR_SLOTS];

int art_pack_head(const uint8_t* p, uint32_t n, uint32_t length);
int art_pack_load(scene_rmr_art* a, uint32_t length, uint32_t crc);
int art_pack_open(scene_rmr_art* a, const store* st, store_file* f);
int art_pack_bytes(scene_rmr_art* a, const uint8_t* p, uint32_t length);

#ifdef __cplusplus
}
#endif

#endif
