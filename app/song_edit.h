// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_EDIT_H
#define SIDOU_SONG_EDIT_H

#include "song_transport.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

enum {
    SONG_EDIT_READY, SONG_EDIT_WAITING, SONG_EDIT_TIDYING, SONG_EDIT_SAVING,
    SONG_EDIT_FULL, SONG_EDIT_INVALID, SONG_EDIT_UNAVAILABLE, SONG_EDIT_BUSY
};

typedef struct {
    song_edit_request request;
    uint8_t before[GT2_ROW_BYTES];
    uint8_t locks[SONG_LOCK_PARAMS * SONG_LOCK_BYTES];
    uint8_t nlocks;
} song_edit_record;

typedef struct {
    const song_transport* transport;
    song_edit_record undo, next;
    song_edit_request request;
    gt2_edit edit;
    uint32_t ticket, revision;
    uint8_t pending, prepared, sent, compacting;
    uint8_t can_undo, undone, action, status;
    gt2_err error;
} song_editor;

void song_edit_init(song_editor* editor, const song_transport* transport);
int song_edit_row(song_editor* editor, uint32_t pattern, uint32_t row, const uint8_t cell[4]);
int song_edit_shift(song_editor* editor, uint32_t pattern, uint32_t row, int insert);
int song_edit_undo(song_editor* editor);
int song_edit_poll(song_editor* editor);
int song_edit_read_row(const song_editor* editor, uint32_t pattern, uint32_t row, uint8_t cell[4]);
const char* song_edit_message(const song_editor* editor);

#ifdef __cplusplus
}
#endif

#endif
