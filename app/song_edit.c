// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_edit.h"

#include <string.h>

#define EDIT_NEW 1u
#define EDIT_UNDO 2u
#define EDIT_REDO 3u

static void copy_bytes(uint8_t* to, const uint8_t* from, uint32_t n) {
    for (uint32_t i = 0u; i < n; i++)
        to[i] = from[i];
}

void song_edit_init(song_editor* editor, const song_transport* transport) {
    memset(editor, 0, sizeof(*editor));
    editor->transport = transport;
}

static gt2_song* model(const song_editor* editor) {
    return editor->transport ? (gt2_song*)editor->transport->song : 0;
}

static int guarded(song_editor* editor, const song_edit_request* request) {
    const song_transport* t = editor->transport;
    if (!t || !t->song || !t->edit || !t->edit_done || !t->edit_guard) {
        editor->status = SONG_EDIT_UNAVAILABLE;
        return 0;
    }
    int result = t->edit_guard(t->ctx, request);
    if (result != SONG_EDIT_GUARD_OK) {
        editor->status = result == SONG_EDIT_GUARD_SAVING ? SONG_EDIT_SAVING :
                         result == SONG_EDIT_GUARD_BUSY ? SONG_EDIT_BUSY : SONG_EDIT_FULL;
        return 0;
    }
    return 1;
}

int song_edit_read_row(const song_editor* editor, uint32_t pattern, uint32_t row, uint8_t cell[4]) {
    const gt2_song* song = model(editor);
    if (!song || pattern >= GT2_MAX_PATTERNS || row >= GT2_MAX_ROWS ||
        row + 1u >= gt2_pattern_rows(song, pattern))
        return 0;
    const uint8_t* data = gt2_pattern_data(song, pattern);
    if (!data)
        return 0;
    if (editor->pending && editor->request.kind == SONG_EDIT_ROW &&
        editor->request.pattern == pattern && editor->request.row == row)
        copy_bytes(cell, editor->request.cell, GT2_ROW_BYTES);
    else
        copy_bytes(cell, data + row * GT2_ROW_BYTES, GT2_ROW_BYTES);
    return 1;
}

static void capture(song_editor* editor, const song_edit_request* request) {
    gt2_song* song = model(editor);
    song_edit_record* record = &editor->next;
    record->request = *request;
    uint32_t lost = request->kind == SONG_EDIT_PUSH ? gt2_pattern_rows(song, request->pattern) - 2u :
                                                   request->row;
    copy_bytes(record->before, gt2_pattern_data(song, request->pattern) + lost * GT2_ROW_BYTES,
               GT2_ROW_BYTES);
    record->nlocks = 0u;
    if (request->kind != SONG_EDIT_ROW) {
        uint32_t n = 0u;
        const uint8_t* locks = song_locks_row(song, request->pattern, lost, &n);
        if (n > SONG_LOCK_PARAMS)
            n = SONG_LOCK_PARAMS;
        copy_bytes(record->locks, locks, n * SONG_LOCK_BYTES);
        record->nlocks = (uint8_t)n;
    }
}

static int start(song_editor* editor, const song_edit_request* request, uint8_t action) {
    if (editor->pending)
        return 0;
    if (!guarded(editor, request))
        return 0;
    if (action == EDIT_UNDO && editor->undo.nlocks && !editor->transport->edit_with_locks) {
        editor->status = SONG_EDIT_UNAVAILABLE;
        return 0;
    }
    gt2_song* song = model(editor);
    if (request->pattern >= GT2_MAX_PATTERNS || request->row >= GT2_MAX_ROWS ||
        request->row + 1u >= gt2_pattern_rows(song, request->pattern) ||
        !gt2_pattern_data(song, request->pattern)) {
        editor->status = SONG_EDIT_INVALID;
        editor->error = GT2_ERR_RANGE;
        return 0;
    }
    editor->request = *request;
    editor->action = action;
    editor->prepared = editor->sent = editor->compacting = 0u;
    editor->pending = 1u;
    editor->status = SONG_EDIT_WAITING;
    editor->error = GT2_OK;
    if (request->kind == SONG_EDIT_ROW) {
        song_edit_poll(editor);
    }
    return editor->pending || editor->status == SONG_EDIT_READY;
}

int song_edit_row(song_editor* editor, uint32_t pattern, uint32_t row, const uint8_t cell[4]) {
    song_edit_request request = {.kind = SONG_EDIT_ROW, .pattern = pattern, .row = row};
    copy_bytes(request.cell, cell, GT2_ROW_BYTES);
    return start(editor, &request, EDIT_NEW);
}

int song_edit_shift(song_editor* editor, uint32_t pattern, uint32_t row, int insert) {
    song_edit_request request = {
        .kind = insert ? SONG_EDIT_PUSH : SONG_EDIT_PULL, .pattern = pattern, .row = row
    };
    return start(editor, &request, EDIT_NEW);
}

int song_edit_undo(song_editor* editor) {
    if (!editor->can_undo || editor->pending)
        return 0;
    song_edit_request request = editor->undo.request;
    uint8_t action = editor->undone ? EDIT_REDO : EDIT_UNDO;
    if (action == EDIT_UNDO) {
        if (request.kind == SONG_EDIT_ROW)
            copy_bytes(request.cell, editor->undo.before, GT2_ROW_BYTES);
        else
            request.kind = request.kind == SONG_EDIT_PUSH ? SONG_EDIT_PULL : SONG_EDIT_PUSH;
    }
    return start(editor, &request, action);
}

static gt2_err prepare(song_editor* editor) {
    gt2_song* song = model(editor);
    const song_edit_request* request = &editor->request;
    if (editor->action == EDIT_NEW)
        capture(editor, request);
    if (request->kind == SONG_EDIT_ROW)
        return gt2_set_row(song, request->pattern, request->row, request->cell, &editor->edit);
    if (editor->action == EDIT_UNDO)
        return gt2_pattern_shift_restore(song, request->pattern, request->row,
                                         request->kind == SONG_EDIT_PUSH, editor->undo.before,
                                         editor->undo.locks, editor->undo.nlocks, &editor->edit);
    if (request->kind == SONG_EDIT_PUSH)
        return gt2_pattern_push_row(song, request->pattern, request->row, &editor->edit);
    else
        return gt2_pattern_pull_row(song, request->pattern, request->row, &editor->edit);
}

static int finish(song_editor* editor) {
    if (editor->compacting) {
        editor->compacting = 0u;
        return 0;
    }
    if (editor->action == EDIT_NEW) {
        editor->undo = editor->next;
        editor->can_undo = 1u;
        editor->undone = 0u;
    } else {
        editor->undone = editor->action == EDIT_UNDO;
    }
    editor->pending = 0u;
    editor->sent = editor->prepared = 0u;
    editor->status = SONG_EDIT_READY;
    editor->revision++;
    return 1;
}

int song_edit_poll(song_editor* editor) {
    if (!editor->pending)
        return 0;
    const song_transport* t = editor->transport;
    if (editor->sent) {
        if (!t->edit_done(t->ctx, editor->ticket))
            return 0;
        int changed = finish(editor);
        if (!editor->pending || editor->status == SONG_EDIT_FULL)
            return changed;
        editor->sent = editor->prepared = 0u;
        return 0;
    }
    if (!guarded(editor, &editor->request)) {
        if (editor->status == SONG_EDIT_SAVING && editor->prepared)
            editor->status = SONG_EDIT_WAITING;
        else {
            if (editor->status != SONG_EDIT_SAVING) {
                if (editor->prepared)
                    gt2_edit_cancel(model(editor), &editor->edit);
                editor->pending = editor->prepared = editor->compacting = 0u;
                if (editor->status == SONG_EDIT_FULL)
                    editor->error = GT2_ERR_FULL;
            }
            return 0;
        }
    }
    if (!editor->prepared) {
        gt2_err result = prepare(editor);
        editor->error = result;
        if (result == GT2_ERR_FRAGMENTED) {
            editor->status = SONG_EDIT_TIDYING;
            if (gt2_compact_step(model(editor), &editor->edit) <= 0)
                return 0;
            editor->compacting = 1u;
        } else if (result == GT2_ERR_PENDING) {
            editor->status = SONG_EDIT_WAITING;
            return 0;
        } else if (result == GT2_ERR_SAVING) {
            editor->status = SONG_EDIT_SAVING;
            return 0;
        } else if (result != GT2_OK) {
            editor->status = result == GT2_ERR_FULL || result == GT2_ERR_SONG_FULL ?
                                 SONG_EDIT_FULL : SONG_EDIT_INVALID;
            editor->pending = 0u;
            return 0;
        }
        editor->prepared = 1u;
    }
    int accepted;
    if (!editor->compacting && editor->action == EDIT_UNDO && editor->undo.nlocks)
        accepted = t->edit_with_locks(t->ctx, &editor->edit, editor->undo.locks,
                                       editor->undo.nlocks, &editor->ticket);
    else
        accepted = t->edit(t->ctx, &editor->edit, &editor->ticket);
    if (accepted)
        editor->sent = 1u;
    if (!editor->compacting)
        editor->status = SONG_EDIT_WAITING;
    return 0;
}

const char* song_edit_message(const song_editor* editor) {
    switch (editor->status) {
    case SONG_EDIT_WAITING: return "WAITING";
    case SONG_EDIT_TIDYING: return "TIDYING";
    case SONG_EDIT_SAVING: return "SAVING";
    case SONG_EDIT_FULL: return "SONG FULL";
    case SONG_EDIT_INVALID: return "INVALID EDIT";
    case SONG_EDIT_UNAVAILABLE: return "NO SONG";
    case SONG_EDIT_BUSY: return "SONG BUSY";
    default: return "";
    }
}
