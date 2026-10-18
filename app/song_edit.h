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

/* A shift's inverse needs only the row it discarded and that row's locks. */
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
/* 1: retained (possibly awaiting transport room or compaction), 0: refused. No further
 * edit is accepted while pending. Poll after song_ctl_poll; revision changes only after
 * completion. The caller must finish pending work before rebinding/loading the song. */
int song_edit_row(song_editor* editor, uint32_t pattern, uint32_t row, const uint8_t cell[4]);
int song_edit_shift(song_editor* editor, uint32_t pattern, uint32_t row, int insert);
/* One history entry: undo, then redo, alternating. */
int song_edit_undo(song_editor* editor);
/* At most one rebuild/compaction step per poll. Rebuild preparation runs here, not in the
 * input handler; row writes may be prepared immediately. SAVING blocks new preparation,
 * not submission of an edit the model already accepted. 1: a user edit completed. */
int song_edit_poll(song_editor* editor);
/* Current row, including an unacknowledged row write (for consecutive knob changes).
 * Returns 0 for an invalid/unheld row. Shift previews wait for completion. */
int song_edit_read_row(const song_editor* editor, uint32_t pattern, uint32_t row, uint8_t cell[4]);
const char* song_edit_message(const song_editor* editor);

#ifdef __cplusplus
}
#endif

#endif
