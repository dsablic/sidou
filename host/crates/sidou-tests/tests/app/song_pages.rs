// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Page editing and integrated routing tests. The fake
//! holds every edit until an explicit audio-boundary acknowledgement, catching live writes.

use std::ffi::{c_void, CStr};

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model};

use crate::views::Ui;

const PANEL_ENC_K2: u32 = SONG_PAGES_ENC_K2;
const PANEL_ENC_PRESETS: u32 = SONG_PAGES_ENC_PRESETS;

#[derive(Default)]
struct Queue {
    edit: Option<gt2_edit>,
    submitted: u32,
    completed: u32,
    guard: i32,
    refuse: bool,
    kinds: Vec<u8>,
    /// The drafts the ARP helper gave the player: voice, first row, left and right bytes.
    drafts: Vec<(u32, u32, Vec<u8>, Vec<u8>)>,
}

unsafe extern "C" fn hear(
    ctx: *mut c_void,
    voice: u32,
    first: u32,
    n: u32,
    left: *const u8,
    right: *const u8,
) -> i32 {
    let q = unsafe { &mut *ctx.cast::<Queue>() };
    let (l, r) = if n == 0 {
        (Vec::new(), Vec::new())
    } else {
        unsafe {
            (
                std::slice::from_raw_parts(left, n as usize).to_vec(),
                std::slice::from_raw_parts(right, n as usize).to_vec(),
            )
        }
    };
    q.drafts.push((voice, first, l, r));
    1
}

unsafe extern "C" fn submit(ctx: *mut c_void, edit: *const gt2_edit, ticket: *mut u32) -> i32 {
    let q = unsafe { &mut *ctx.cast::<Queue>() };
    if q.refuse || q.edit.is_some() {
        return 0;
    }
    let edit = unsafe { *edit };
    q.kinds.push(edit.kind);
    q.edit = Some(edit);
    q.submitted += 1;
    unsafe { *ticket = q.submitted };
    1
}

unsafe extern "C" fn done(ctx: *mut c_void, ticket: u32) -> i32 {
    let q = unsafe { &*ctx.cast::<Queue>() };
    i32::from(q.completed >= ticket)
}

unsafe extern "C" fn guard(ctx: *mut c_void, _: *const song_edit_request) -> i32 {
    unsafe { (*ctx.cast::<Queue>()).guard }
}

unsafe extern "C" fn status(_: *mut c_void, out: *mut song_status) {
    unsafe { *out = song_status::default() };
}

struct Pages {
    ui: Ui,
    p: Box<song_pages>,
    q: Box<Queue>,
    model: Model,
    _transport: Box<song_transport>,
}

impl Pages {
    fn new(v: &'static view) -> Self {
        let mut ui = Ui::without_cards();
        let model = Model::device();
        let mut q = Box::<Queue>::default();
        let transport = Box::new(song_transport {
            song: model.ptr(),
            ctx: (&mut *q as *mut Queue).cast(),
            edit: Some(submit),
            edit_done: Some(done),
            edit_guard: Some(guard),
            status: Some(status),
            draft: Some(hear),
            ..Default::default()
        });
        let mut p = Box::<song_pages>::default();
        crate::drain(&mut ui.g);
        unsafe {
            song_view_set_transport(&mut ui.v.song, &*transport);
            song_pages_init(&mut *p, &mut ui.v.song);
            screen_push(
                &mut ui.v.s,
                v,
                (&mut *p as *mut song_pages).cast(),
                SCREEN_CUT,
            );
        }
        Self {
            ui,
            p,
            q,
            model,
            _transport: transport,
        }
    }

    fn input(&mut self, kind: u32, id: u32, steps: i16, held: u32) {
        let event = screen_event {
            kind: kind as u8,
            id: id as u8,
            steps,
            held,
        };
        let v = unsafe { &*screen_top(&self.ui.v.s) };
        assert_eq!(
            unsafe {
                v.input.unwrap()(
                    &mut self.ui.v.s,
                    (&mut *self.p as *mut song_pages).cast(),
                    &event,
                )
            },
            1
        );
    }

    fn turn(&mut self, id: u32, steps: i16) {
        self.input(SCREEN_TURN, id, steps, 0);
    }

    fn popup(&self, encoder: u32, edit: bool) -> Option<(String, String, u32, u32)> {
        let mut name = [0; SCREEN_POPUP_TEXT as usize];
        let mut value = [0; SCREEN_POPUP_TEXT as usize];
        let (mut fill, mut max) = (0, 0);
        let supported = unsafe {
            song_pages_popup(
                &*self.p,
                screen_top(&self.ui.v.s),
                encoder,
                i32::from(edit),
                name.as_mut_ptr(),
                value.as_mut_ptr(),
                &mut fill,
                &mut max,
            )
        };
        (supported != 0).then(|| {
            (
                unsafe { CStr::from_ptr(name.as_ptr()) }
                    .to_str()
                    .unwrap()
                    .to_owned(),
                unsafe { CStr::from_ptr(value.as_ptr()) }
                    .to_str()
                    .unwrap()
                    .to_owned(),
                fill,
                max,
            )
        })
    }

    fn finish(&mut self) {
        for _ in 0..32 {
            self.ack();
            unsafe { song_pages_poll(&mut *self.p) };
            if self.p.pending == 0 {
                return;
            }
        }
        panic!("page edit did not finish");
    }

    fn ack(&mut self) {
        if let Some(e) = self.q.edit.take() {
            unsafe { gt2_edit_commit(self.model.ptr_mut(), &e) };
            self.q.completed = self.q.submitted;
        }
    }

    fn order(&self) -> Vec<u8> {
        let n = unsafe { gt2_order_entries(self.model.ptr(), u32::from(self.p.subtune), 0) };
        unsafe {
            std::slice::from_raw_parts(
                gt2_order_data(self.model.ptr(), u32::from(self.p.subtune), 0),
                n as usize + 2,
            )
            .to_vec()
        }
    }

    fn settle(&mut self) {
        for n in 0..40 {
            unsafe { screen_frame(&mut self.ui.v.s, n * 40) };
            let sent = crate::drain(&mut self.ui.g);
            assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
            assert!(sent.spi_ms() <= crate::SPI_BUDGET_MS, "{sent:?}");
            if self.p.unbuilt == 0 && self.ui.v.song.dirty == 0 {
                return;
            }
        }
        panic!("page did not settle");
    }
}

#[test]
fn instrument_coarse_fine_and_names_use_acknowledged_cells() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    let old = h.model.raw().instr[0][0];
    h.turn(PANEL_ENC_K1, 1);
    assert_eq!(h.model.raw().instr[0][0], old, "not a live write");
    h.finish();
    assert_eq!(h.model.raw().instr[0][0], old.saturating_add(16));
    h.turn(PANEL_ENC_K2, -1);
    h.finish();
    assert_eq!(h.model.raw().instr[0][0], old.saturating_add(16) - 1);
    h.turn(PANEL_ENC_SELECT, 9);
    h.turn(PANEL_ENC_K2, 1);
    h.finish();
    assert_eq!(h.model.raw().instr[0][GT2_INS_NAME as usize], b'!');
    h.turn(PANEL_ENC_PRESETS, 1);
    assert_eq!(h.ui.v.song.instr, 2);
    assert!(h.q.kinds.iter().all(|&k| u32::from(k) == GT2_EDIT_CELL));
}

#[test]
fn parameter_edits_preserve_all_sixteen_imported_instrument_name_bytes() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    let mut spec = SngSpec::new();
    let mut instrument = [0; GT2_INSTR_BYTES as usize];
    instrument[0] = 0x10;
    let name = [
        0, 0xff, b'a', b' ', 0x7f, b'Z', 1, 0x80, b'1', 0xfe, b'_', 0x1f, b'B', 0, 0xc0, b'!',
    ];
    instrument[GT2_INS_NAME as usize..].copy_from_slice(&name);
    spec.instr = vec![instrument];
    h.model.read(&spec.encode(), 0).unwrap();
    h.turn(PANEL_ENC_K2, 1);
    h.finish();
    assert_eq!(h.model.raw().instr[0][0], 0x11);
    assert_eq!(h.model.raw().instr[0][GT2_INS_NAME as usize..], name);
}

#[test]
fn order_creates_a_default_pattern_before_publishing_its_reference() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    h.turn(PANEL_ENC_K1, 3);
    assert_eq!(h.order(), [0, 0xff, 0]);
    assert_eq!(
        h.p.prepared, 0,
        "pattern creation waits for the bounded poll"
    );
    assert!(h.q.edit.is_none());
    unsafe { song_pages_poll(&mut *h.p) };
    assert_eq!(h.q.edit.as_ref().unwrap().kind, GT2_EDIT_SWAP as u8);
    h.finish();
    assert_eq!(h.order(), [3, 0xff, 0]);
    assert_eq!(
        unsafe { gt2_pattern_rows(h.model.ptr(), 3) },
        GT2_DEFAULT_ROWS + 1
    );
    assert!(!unsafe { gt2_pattern_data(h.model.ptr(), 3) }.is_null());
    assert_eq!(h.q.kinds, [GT2_EDIT_SWAP as u8, GT2_EDIT_CELL as u8]);
}

#[test]
fn a_skipped_pattern_number_does_not_materialize_the_gaps() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    h.turn(PANEL_ENC_K1, 9);
    assert_eq!(h.model.raw().pattern[9], 0, "the index waits for its ack");
    assert_eq!(h.p.prepared, 0);
    unsafe { song_pages_poll(&mut *h.p) };
    assert_ne!(h.model.raw().pend_pattern[9], 0);
    assert_eq!(h.model.raw().pend_pattern[3], 0);
    h.finish();
    assert_eq!(h.order(), [9, 0xff, 0]);
    assert!(h.model.raw().pattern[3..9].iter().all(|&span| span == 0));
    assert!(h
        .model
        .raw()
        .pend_pattern
        .iter()
        .all(|&pending| pending == 0));
    assert_eq!(h.model.raw().pending, 0);
}

#[test]
fn order_insert_restart_delete_and_invalid_last_kind() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    let edit = 1 << PANEL_BTN_EDIT;
    h.input(SCREEN_TURN, PANEL_ENC_PRESETS, 1, edit);
    h.finish();
    assert_eq!(h.order(), [0, 0, 0xff, 1]);
    h.turn(PANEL_ENC_SELECT, 1);
    h.input(SCREEN_PRESS, PANEL_BTN_ARP, 0, 0);
    h.finish();
    assert_eq!(h.order(), [0, 0, 0xff, 1]);
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(
        h.p.status, SONG_PAGES_INVALID as u8,
        "last must remain a pattern"
    );
    assert_eq!(h.model.raw().pending, 0);
    h.input(SCREEN_TURN, PANEL_ENC_PRESETS, -1, edit);
    h.finish();
    assert_eq!(h.order(), [0, 0xff, 0]);
}

#[test]
fn order_subtune_selector_and_voice_navigation_are_separate() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    let mut spec = SngSpec::new();
    spec.orders.push(spec.orders[0].clone());
    h.model.read(&spec.encode(), 0).unwrap();
    h.turn(PANEL_ENC_SELECT, -1);
    assert_eq!(h.p.order_cursor, 0);
    h.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(h.p.subtune, 1);
    assert_eq!(h.ui.v.song.voice, 0);
    h.turn(PANEL_ENC_SELECT, 1);
    h.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(h.ui.v.song.voice, 1);
    assert_eq!(h.ui.v.song.order_pos, 0);
}

#[test]
fn table_bytes_are_separate_cells_and_pointer_jump_is_one_based() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.turn(PANEL_ENC_K1, 0x41);
    assert_eq!(h.model.raw().ltable[0][0], 0);
    h.finish();
    h.turn(PANEL_ENC_K2, 0x80);
    h.finish();
    assert_eq!(h.model.raw().ltable[0][0], 0x41);
    assert_eq!(h.model.raw().rtable[0][0], 0x80);
    h.turn(PANEL_ENC_ALGORITHM, 3);
    assert_eq!(h.p.table, GT2_STBL as u8);
    h.turn(PANEL_ENC_SELECT, 1000);
    assert_eq!(u32::from(h.p.table_row), GT2_TABLE_ROWS - 1);
    unsafe { screen_pop(&mut h.ui.v.s, SCREEN_CUT) };
    unsafe {
        screen_push(
            &mut h.ui.v.s,
            &song_pages_instr_view,
            (&mut *h.p as *mut song_pages).cast(),
            SCREEN_CUT,
        );
    }
    h.turn(PANEL_ENC_SELECT, GT2_INS_PULSE as i16);
    h.turn(PANEL_ENC_K2, 5);
    h.finish();
    let e = screen_event {
        kind: SCREEN_PRESS as u8,
        id: PANEL_BTN_SEQ as u8,
        held: 1 << PANEL_BTN_EDIT,
        ..Default::default()
    };
    assert_eq!(unsafe { song_pages_jump(&mut *h.p, &mut h.ui.v.s, &e) }, 1);
    assert_eq!(h.p.table, GT2_PTBL as u8);
    assert_eq!(h.p.table_row, 4);
    assert_eq!(unsafe { screen_top(&h.ui.v.s) }, unsafe {
        &song_pages_table_view as *const view
    });
}

#[test]
fn save_guard_and_full_queue_never_write_live_bytes_or_leak_pending_edits() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    h.q.guard = SONG_EDIT_GUARD_SAVING as i32;
    let before = h.model.bytes().to_vec();
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(h.model.bytes(), before);
    assert_eq!(h.p.status, SONG_PAGES_SAVING as u8);
    h.q.guard = SONG_EDIT_GUARD_OK as i32;
    h.q.refuse = true;
    h.turn(PANEL_ENC_K2, 1);
    assert!(h.q.edit.is_none());
    assert_eq!(h.model.raw().pending_loose, 1);
    h.q.refuse = false;
    h.finish();
    assert_eq!(h.model.raw().pending_loose, 0);
    assert_eq!(h.p.revision, 1);
}

#[test]
fn terminal_guard_refusals_cancel_unsubmitted_cells_and_swaps() {
    for (unavailable, guard_value, status_value) in [
        (true, SONG_EDIT_GUARD_OK, SONG_PAGES_UNAVAILABLE),
        (false, SONG_EDIT_GUARD_FULL, SONG_PAGES_FULL),
        (false, SONG_EDIT_GUARD_BUSY, SONG_PAGES_BUSY),
    ] {
        for v in unsafe { [&song_pages_instr_view, &song_pages_order_view] } {
            let mut h = Pages::new(v);
            let used = h.model.raw().used_units;
            let order = h.order();
            let instruments = h.model.raw().instr;
            let patterns = h.model.raw().pattern;
            h.q.refuse = true;
            if std::ptr::eq(v, unsafe { &song_pages_order_view }) {
                h.input(SCREEN_TURN, PANEL_ENC_PRESETS, 1, 1 << PANEL_BTN_EDIT);
                assert_eq!(h.p.prepared, 0, "ORDER rebuilds wait for poll");
                unsafe { song_pages_poll(&mut *h.p) };
                assert!(h.model.raw().used_units > used);
            } else {
                h.turn(PANEL_ENC_K2, 1);
                assert_eq!(h.model.raw().pending_loose, 1);
            }
            assert_eq!(h.p.pending, 1);
            assert_eq!(h.p.prepared, 1);
            assert_eq!(h.p.sent, 0);
            if unavailable {
                h._transport.edit_guard = None;
            } else {
                h.q.guard = guard_value as i32;
            }
            unsafe { song_pages_poll(&mut *h.p) };
            assert_eq!(h.p.pending, 0);
            assert_eq!(h.p.prepared, 0);
            assert_eq!(h.p.sent, 0);
            assert_eq!(h.p.auxiliary, 0);
            assert_eq!(h.p.status, status_value as u8);
            assert_eq!(h.p.revision, 0);
            assert_eq!(
                h.model.raw().used_units,
                used,
                "refusal must free its private copy"
            );
            assert_eq!(h.model.raw().pending, 0);
            assert_eq!(h.model.raw().pending_loose, 0);
            assert_eq!(h.model.raw().pending_orders, 0);
            assert_eq!(h.model.raw().pending_old_units, 0);
            assert_eq!(h.model.raw().instr, instruments);
            assert_eq!(h.model.raw().pattern, patterns);
            assert_eq!(h.order(), order);
            h._transport.edit_guard = Some(guard);
            h.q.guard = SONG_EDIT_GUARD_OK as i32;
            h.q.refuse = false;
            h.turn(PANEL_ENC_K1, 1);
            h.finish();
            assert_eq!(
                h.p.revision, 1,
                "terminal refusal must not strand the editor"
            );
        }
    }
}

#[test]
fn a_prepared_reservation_drains_while_saving_to_avoid_a_save_wait_deadlock() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    let before = h.model.raw().instr[0][0];
    h.q.refuse = true;
    h.turn(PANEL_ENC_K2, 1);
    h.q.guard = SONG_EDIT_GUARD_SAVING as i32;
    unsafe { (*h.model.ptr_mut()).frozen = 1 };
    h.q.refuse = false;
    unsafe { song_pages_poll(&mut *h.p) };
    assert_eq!(h.p.status, SONG_PAGES_WAITING as u8);
    assert_eq!(h.p.pending, 1);
    assert_eq!(h.p.prepared, 1);
    assert_eq!(h.model.raw().pending_loose, 1);
    assert!(h.q.edit.is_some());
    assert_eq!(h.model.raw().instr[0][0], before);
    h.finish();
    assert_eq!(h.model.raw().instr[0][0], before + 1);
    assert_eq!(h.model.raw().pending_loose, 0);
    assert_eq!(h.p.revision, 1);
    unsafe { (*h.model.ptr_mut()).frozen = 0 };
}

#[test]
fn accepted_completions_do_not_recheck_the_prospective_guard() {
    for unavailable in [false, true] {
        let mut h = Pages::new(unsafe { &song_pages_instr_view });
        let before = h.model.raw().instr[0][0];
        h.turn(PANEL_ENC_K2, 1);
        assert_eq!(h.p.sent, 1);
        if unavailable {
            h._transport.edit_guard = None;
        } else {
            h.q.guard = SONG_EDIT_GUARD_FULL as i32;
        }
        h.ack();
        assert_eq!(unsafe { song_pages_poll(&mut *h.p) }, 1);
        assert_eq!(h.p.pending, 0);
        assert_eq!(h.p.status, SONG_PAGES_READY as u8);
        assert_eq!(h.p.revision, 1);
        assert_eq!(h.model.raw().instr[0][0], before + 1);
        assert_eq!(h.model.raw().pending_loose, 0);
    }
}

#[test]
fn input_acknowledges_sent_edits_before_reading_the_next_detents_value() {
    for v in unsafe {
        [
            &song_pages_order_view,
            &song_pages_instr_view,
            &song_pages_table_view,
        ]
    } {
        let mut h = Pages::new(v);
        h.turn(PANEL_ENC_K1, 1);
        let first = h.q.edit.as_ref().unwrap().bytes[0];
        h.ack();
        assert_eq!(
            h.p.pending, 1,
            "the page has not polled the acknowledgement yet"
        );
        h.turn(PANEL_ENC_K1, 1);
        assert_eq!(h.p.revision, 1);
        assert_eq!(h.q.submitted, 2, "the second detent must not be dropped");
        let step = if std::ptr::eq(v, unsafe { &song_pages_instr_view }) {
            16
        } else {
            1
        };
        assert_eq!(h.q.edit.as_ref().unwrap().bytes[0], first + step);
        h.finish();
        assert_eq!(h.p.revision, 2);
    }
}

#[test]
fn input_does_not_submit_an_older_unsent_edit_before_handling_a_detent() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    h.q.refuse = true;
    h.turn(PANEL_ENC_K2, 1);
    h.q.refuse = false;
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(h.q.submitted, 0);
    assert_eq!(h.p.prepared, 1);
    assert_eq!(h.p.sent, 0);
    h.finish();
    assert_eq!(h.p.revision, 1);
}

#[test]
fn order_popup_previews_the_selected_entry_value_and_kind_not_live_parameters() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    h.turn(PANEL_ENC_K1, 1);
    assert_eq!(h.order()[0], 0);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("PATTERN".into(), "01".into(), 1, GT2_MAX_PATTERNS - 1))
    );
    assert_eq!(
        h.popup(PANEL_ENC_K2, false),
        Some(("ORDER KIND".into(), "PATTERN".into(), 0, 0))
    );
    h.finish();
    h.input(SCREEN_TURN, PANEL_ENC_PRESETS, 1, 1 << PANEL_BTN_EDIT);
    h.finish();
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(
        h.popup(PANEL_ENC_K2, false),
        Some(("ORDER KIND".into(), "REPEAT".into(), 0, 0))
    );
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("REPEAT".into(), "R2".into(), 1, 15))
    );
    h.finish();
    h.turn(PANEL_ENC_K1, 14);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("REPEAT".into(), "R16".into(), 15, 15))
    );
    h.finish();
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(
        h.popup(PANEL_ENC_K2, false),
        Some(("ORDER KIND".into(), "TRANSPOSE".into(), 0, 0))
    );
    h.finish();
    h.turn(PANEL_ENC_K1, -16);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("TRANSPOSE".into(), "-10".into(), 0, 30))
    );
    h.finish();
    h.turn(PANEL_ENC_SELECT, 2);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("RESTART".into(), "01".into(), 1, 1))
    );
    h.turn(PANEL_ENC_K1, -1);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("RESTART".into(), "00".into(), 0, 1))
    );
    assert!(h.popup(PANEL_ENC_K2, false).is_none());
    h.finish();
    h.turn(PANEL_ENC_SELECT, -3);
    assert!(h.popup(PANEL_ENC_K1, false).is_none());
    assert!(h.popup(PANEL_ENC_PRESETS, true).is_none());
}

#[test]
fn instrument_popup_previews_pending_bytes_and_name_characters_with_bounded_labels() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    h.q.refuse = true;
    h.turn(PANEL_ENC_K1, 1);
    let expected = h.p.request.cell[0];
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some((
            "ATTACK/DECAY".into(),
            format!("{expected:02X}"),
            u32::from(expected),
            255
        ))
    );
    assert_eq!(h.popup(PANEL_ENC_K2, true), h.popup(PANEL_ENC_K1, false));
    assert_eq!(h.q.submitted, 0, "popup must not poll or submit an edit");
    h.q.refuse = false;
    h.finish();
    h.turn(PANEL_ENC_SELECT, 1);
    let popup = h.popup(PANEL_ENC_K2, false).unwrap();
    assert_eq!(popup.0, "SUSTAIN/REL");
    assert!(popup.0.len() < SCREEN_POPUP_TEXT as usize);
    h.turn(PANEL_ENC_SELECT, 8);
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(
        h.popup(PANEL_ENC_K2, false),
        Some(("NAME CHAR".into(), "'!'".into(), 1, 94))
    );
    h.finish();
    h.turn(PANEL_ENC_PRESETS, 1);
    assert_eq!(
        h.popup(PANEL_ENC_PRESETS, false),
        Some(("INSTRUMENT".into(), "02".into(), 1, GT2_MAX_INSTR - 1))
    );
    assert_eq!(
        h.popup(PANEL_ENC_PRESETS, true),
        h.popup(PANEL_ENC_PRESETS, false)
    );
    assert_eq!(
        h.popup(PANEL_ENC_K2, false).unwrap().1,
        "00",
        "no preview from another instrument"
    );
    assert!(h.popup(PANEL_ENC_ALGORITHM, false).is_none());
}

#[test]
fn table_popup_previews_only_the_matching_side_row_and_table() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.turn(PANEL_ENC_K1, 0x41);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("LEFT BYTE".into(), "41".into(), 0x41, 255))
    );
    assert_eq!(
        h.popup(PANEL_ENC_K2, true),
        Some(("RIGHT BYTE".into(), "00".into(), 0, 255))
    );
    h.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(h.popup(PANEL_ENC_K1, false).unwrap().1, "00");
    h.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(
        h.popup(PANEL_ENC_ALGORITHM, false),
        Some(("TABLE".into(), "PULSE".into(), 0, 0))
    );
    assert_eq!(h.popup(PANEL_ENC_K1, false).unwrap().1, "00");
    assert_eq!(h.q.submitted, 1, "popup does not advance a pending edit");
    h.finish();
    assert!(h.popup(PANEL_ENC_K4, false).is_none());
    assert!(h.popup(PANEL_ENC_SELECT, false).is_none());
}

#[test]
fn pages_build_in_slices_share_the_canvas_and_become_idle() {
    for v in unsafe {
        [
            &song_pages_order_view,
            &song_pages_instr_view,
            &song_pages_table_view,
        ]
    } {
        let mut h = Pages::new(v);
        h.settle();
        let changed = v.changed.unwrap();
        assert_eq!(
            unsafe { changed(&mut h.ui.v.s, (&mut *h.p as *mut song_pages).cast()) },
            0
        );
        assert_eq!(h.ui.v.song.cv.ch.len(), GRID_ROWS as usize);
        let before = h.ui.v.song.cv;
        h.ui.v.s.composed = 0;
        unsafe { screen_frame(&mut h.ui.v.s, 2000) };
        assert_eq!(h.ui.v.s.composed, 0, "idle page must not compose");
        assert_eq!(h.ui.v.song.cv.ch, before.ch);
    }
}

#[test]
fn speed_ff_is_a_speed_not_a_jump() {
    let mut text = [0; 31];
    unsafe { song_pages_table_meaning(GT2_STBL, 0xff, 5, text.as_mut_ptr()) };
    let meaning = unsafe { CStr::from_ptr(text.as_ptr()) }.to_str().unwrap();
    assert!(!meaning.contains("JUMP"));
    unsafe { song_pages_table_meaning(GT2_WTBL, 0xff, 5, text.as_mut_ptr()) };
    assert_eq!(
        unsafe { CStr::from_ptr(text.as_ptr()) }.to_str(),
        Ok("JUMP 05")
    );
    unsafe { song_pages_table_meaning(GT2_WTBL, 0x41, 0xa4, text.as_mut_ptr()) };
    assert_eq!(
        unsafe { CStr::from_ptr(text.as_ptr()) }.to_str(),
        Ok("WAVE 41 C-3")
    );
}

#[test]
fn absolute_wavetable_notes_do_not_inherit_the_patterns_reserved_note_codes() {
    let mut text = [0; 31];
    for (right, expected) in [
        (0xdd, "WAVE 41 A-7"),
        (0xdf, "WAVE 41 B-7"),
        (0xe0, "WAVE 41 ABS 60"),
        (0x80, "WAVE 41 KEEP"),
    ] {
        unsafe { song_pages_table_meaning(GT2_WTBL, 0x41, right, text.as_mut_ptr()) };
        assert_eq!(
            unsafe { CStr::from_ptr(text.as_ptr()) }.to_str(),
            Ok(expected)
        );
    }
}

#[test]
fn transpose_minus_sixteen_is_not_displayed_as_minus_zero() {
    let mut text = [0; 4];
    unsafe { tracker_view_order_entry(GT2_TRANSDOWN as u8, text.as_mut_ptr()) };
    assert_eq!(unsafe { CStr::from_ptr(text.as_ptr()) }.to_str(), Ok("-10"));
}

#[test]
fn three_character_transposes_fit_primary_order_text_and_tracker_footer() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut spec = SngSpec::new();
    spec.orders[0] = vec![
        (vec![0, GT2_TRANSDOWN as u8, 1], 0),
        (vec![GT2_TRANSDOWN as u8, 1], 0),
        (vec![2], 0),
    ];
    let mut model = Model::device();
    model.read(&spec.encode(), 0).unwrap();
    let mut h = Song::playing(Some(model));
    open_tracker(&mut h.ui);
    assert_eq!(
        h.ui.small(TRACKER_VIEW_ORDER_ROW as usize, 3, 10),
        "00 -10 01 "
    );
    h.ui.press(PANEL_BTN_SEQ);
    h.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut h.ui);
    assert_eq!(h.ui.big(5, 5, 2, 3), "-10");
    assert_eq!(
        h.ui.big(3, 18, 2, 2),
        "E0",
        "the other voices' entries use two raw hex digits"
    );
}

#[test]
fn page_values_and_parameter_names_are_readable_at_two_times_the_font() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    h.settle();
    assert_eq!(h.ui.big(0, 0, 2, 5), "ORDER");
    assert_eq!(h.ui.big(0, 12, 2, 2), "V1");
    assert_eq!(h.ui.big(3, 0, 2, 2), "00", "the row's number at 2x");
    assert_eq!(h.ui.big(3, 5, 2, 2), "00");
    assert_eq!(h.ui.big(5, 0, 2, 2), "01");
    assert_eq!(h.ui.big(5, 5, 2, 6), "RST 00");
    assert_eq!(h.ui.small(2, 18, 2), "V2");
    assert_eq!(h.ui.small(2, 24, 2), "V3");
    assert_eq!(h.ui.big(3, 18, 2, 2), "01", "voice 2's entry at 2x");
    assert_eq!(h.ui.big(5, 18, 2, 2), "RS");
    assert_eq!(
        h.ui.big(28, 0, 2, 10),
        "PATTERN 00",
        "what the entry is, at 2x"
    );
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    h.settle();
    assert_eq!(h.ui.big(3, 0, 2, 7), "ATK/DEC");
    assert_eq!(h.ui.big(19, 0, 2, 8), "1ST WAVE");
    assert_eq!(h.ui.big(28, 0, 2, 12), "ATTACK/DECAY");
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.settle();
    assert_eq!(h.ui.big(0, 0, 2, 4), "WAVE");
    assert_eq!(h.ui.big(0, 12, 2, 2), "K1", "the knobs at 2x");
    assert_eq!(h.ui.big(0, 17, 2, 2), "K2");
    assert_eq!(h.ui.big(3, 0, 2, 2), "01", "the row's number at 2x");
    assert_eq!(h.ui.big(3, 5, 2, 2), "00");
    assert_eq!(h.ui.big(3, 12, 2, 2), "00");
}

#[test]
fn solo_wins_over_mute_and_the_order_voice_header_uses_the_effective_mask() {
    let mut h = Pages::new(unsafe { &song_pages_order_view });
    h.ui.v.song.mute = 1;
    h.ui.v.song.solo = 1;
    h.settle();
    let th = unsafe { &*theme_get(u32::from(h.ui.live().theme)) };
    assert_eq!(h.ui.small(1, 18, 3), "SOL");
    assert_eq!(h.ui.g.attr[12] & 15, th.voice[0]);
    h.ui.v.song.solo = 2;
    h.settle();
    assert_eq!(h.ui.g.attr[12] & 15, th.dim);
}

fn two_subtunes() -> Model {
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(vec![0, 1], 0); 3], vec![(vec![0, 2], 0); 3]];
    spec.patterns = vec![(vec![[GT2_REST as u8, 0, 0, 0]; 8], [0xff, 0, 0, 0]); 3];
    let mut model = Model::device();
    model.read(&spec.encode(), 0).unwrap();
    model
}

#[test]
fn selected_subtune_drives_play_and_tracker_but_playback_uses_the_actual_subtune() {
    use crate::song_view::{open_tracker, settle, Call, Song};
    let mut h = Song::playing(Some(two_subtunes()));
    open_tracker(&mut h.ui);
    h.ui.press(PANEL_BTN_SEQ);
    settle(&mut h.ui);
    h.ui.turn(PANEL_ENC_SELECT, -1);
    h.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(h.ui.v.song.subtune, 1);
    assert_eq!(h.ui.v.pages.subtune, 1);
    assert_eq!(h.ui.v.song.voice, 0);
    h.ui.turn(PANEL_ENC_SELECT, 2);
    h.ui.press(PANEL_BTN_PLAY);
    assert_eq!(h.f.last_subtune, 1);
    assert!(h.calls().contains(&Call::Play(1)));
    h.ui.press(PANEL_BTN_PLAY);
    for _ in 0..4 {
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
    }
    assert_eq!(h.ui.top(), "TRACKER");
    assert_eq!(h.ui.big(0, 5, 2, 6), "PAT 02");
    h.f.st.state = SONG_T_PLAYING as u8;
    h.f.st.order_pos = [0; 3];
    h.f.st.row = [0; 3];
    h.f.st.subtune = 0;
    settle(&mut h.ui);
    assert_eq!(h.ui.small(TRACKER_VIEW_ORDER_ROW as usize, 3, 5), "00 01");
    h.f.st.subtune = 1;
    settle(&mut h.ui);
    assert_eq!(
        h.ui.small(TRACKER_VIEW_ORDER_ROW as usize, 3, 5),
        "00 02",
        "same first pattern and position must still invalidate the order prefix"
    );
    h.ui.v.song.subtune = 0;
    h.ui.press(PANEL_BTN_REC);
    settle(&mut h.ui);
    assert_eq!(
        h.ui.big(0, 5, 2, 6),
        "PAT 00",
        "REC records: the playing subtune's pattern (ergonomics review E03)"
    );
    h.f.st.state = SONG_T_STOPPED as u8;
    settle(&mut h.ui);
    assert_eq!(
        h.ui.big(0, 5, 2, 6),
        "PAT 01",
        "stopped, REC shows the edit cursor: the selected subtune's entry"
    );
}

#[test]
fn global_edit_seq_jumps_to_the_selected_instrument_pointer_before_cycling() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut spec = SngSpec::new();
    spec.instr = vec![[0; GT2_INSTR_BYTES as usize]];
    spec.instr[0][GT2_INS_PULSE as usize] = 5;
    let mut model = Model::device();
    model.read(&spec.encode(), 0).unwrap();
    let mut h = Song::playing(Some(model));
    open_tracker(&mut h.ui);
    for _ in 0..2 {
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
    }
    h.ui.turn(PANEL_ENC_SELECT, GT2_INS_PULSE as i32);
    h.ui.hold(PANEL_BTN_EDIT, true);
    h.ui.press(PANEL_BTN_SEQ);
    h.ui.hold(PANEL_BTN_EDIT, false);
    assert_eq!(h.ui.top(), "TABLE");
    assert_eq!(h.ui.v.pages.table, GT2_PTBL as u8);
    assert_eq!(h.ui.v.pages.table_row, 4);
    settle(&mut h.ui);
    assert_eq!(h.ui.big(0, 0, 2, 5), "PULSE");
}

#[test]
fn load_clears_page_requests_and_cursors_but_preserves_the_displayed_page() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut h = Song::playing(Some(Model::device()));
    open_tracker(&mut h.ui);
    for _ in 0..3 {
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
    }
    h.ui.turn(PANEL_ENC_SELECT, 12);
    h.ui.turn(PANEL_ENC_ALGORITHM, 2);
    h.ui.v.pages.status = SONG_PAGES_INVALID as u8;
    unsafe { views_song_loaded(&mut *h.ui.v) };
    assert_eq!(h.ui.top(), "TABLE");
    assert_eq!(h.ui.v.pages.page, SONG_VIEW_TABLE as u8);
    assert_eq!(h.ui.v.pages.table_row, 0);
    assert_eq!(h.ui.v.pages.table, 0);
    assert_eq!(h.ui.v.pages.order_cursor, 1);
    assert_eq!(h.ui.v.pages.status, SONG_PAGES_READY as u8);
    settle(&mut h.ui);
    assert_eq!(h.ui.big(0, 0, 2, 4), "WAVE");
}

/// Ergonomics review E27 (M2 review 5.10): ABOUT's tune and the user's song coming back
/// (views_song_back) leave the song views' places and choices; what read the song starts again.
#[test]
fn a_returning_song_keeps_the_cursors_lanes_and_instrument() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut h = Song::playing(Some(Model::device()));
    open_tracker(&mut h.ui);
    let sv = &mut h.ui.v.song;
    (sv.voice, sv.instr, sv.order_pos, sv.row, sv.subtune) = (2, 5, 1, 17, 0);
    sv.rec_n = 1;
    sv.built = 1;
    sv.gen_.root = [40, 41, 42];
    let st = &mut h.ui.v.step;
    (st.page, st.gate, st.instr) = (2, 0b101, [3, 4, 6]);
    st.held = 3;
    let pg = &mut h.ui.v.pages;
    (
        pg.order_cursor,
        pg.subtune,
        pg.instr_cursor,
        pg.table,
        pg.table_row,
    ) = (3, 0, 4, 1, 9);
    pg.status = SONG_PAGES_INVALID as u8;
    unsafe { views_song_back(&mut *h.ui.v) };
    let sv = &h.ui.v.song;
    assert_eq!(
        (sv.voice, sv.instr, sv.order_pos, sv.row, sv.subtune),
        (2, 5, 1, 17, 0)
    );
    assert_eq!(
        (sv.rec_n, sv.built),
        (0, 0),
        "REC's queue and the canvas start again"
    );
    let st = &h.ui.v.step;
    assert_eq!(
        (st.page, st.gate, h.ui.v.song.gen_.root, st.instr),
        (2, 0b101, [40, 41, 42], [3, 4, 6])
    );
    assert_eq!(st.held, STEP_VIEW_NONE as u8);
    let pg = &h.ui.v.pages;
    assert_eq!(
        (
            pg.order_cursor,
            pg.subtune,
            pg.instr_cursor,
            pg.table,
            pg.table_row
        ),
        (3, 0, 4, 1, 9)
    );
    assert_eq!(pg.status, SONG_PAGES_READY as u8);
    assert_eq!(h.ui.top(), "TRACKER");
    settle(&mut h.ui);
    // A load proper resets them.
    unsafe { views_song_loaded(&mut *h.ui.v) };
    let sv = &h.ui.v.song;
    assert_eq!((sv.voice, sv.instr, sv.order_pos, sv.row), (0, 1, 0, 0));
    assert_eq!(h.ui.v.song.gen_.root, [36; 3]);
}

#[test]
fn integrated_page_popups_use_page_values_and_unused_knobs_keep_live_behavior() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut h = Song::playing(Some(Model::device()));
    open_tracker(&mut h.ui);
    for _ in 0..2 {
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
    }
    let old = unsafe { (*h.ui.v.song.song).instr[0][0] };
    h.ui.turn(PANEL_ENC_K1, 1);
    settle(&mut h.ui);
    assert_eq!(
        unsafe { CStr::from_ptr(h.ui.v.popup.name.as_ptr()) }.to_str(),
        Ok("ATTACK/DECAY")
    );
    assert_eq!(
        unsafe { (*h.ui.v.song.song).instr[0][0] },
        old.saturating_add(16)
    );
    assert_ne!(h.ui.v.song.which, SONG_VIEW_TRACKER as u8);
    // K3 is the sound's own control since the knob-row change: the decay on the saw preset
    h.ui.turn(PANEL_ENC_K1 + 2, 1);
    settle(&mut h.ui);
    assert_eq!(
        unsafe { CStr::from_ptr(h.ui.v.popup.name.as_ptr()) }.to_str(),
        Ok("DECAY")
    );
}

#[test]
fn a_pending_page_edit_retains_its_binding_and_finishes_after_home() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut h = Song::playing(Some(Model::device()));
    open_tracker(&mut h.ui);
    for _ in 0..2 {
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
    }
    h.f.refuse = true;
    let binding = h.ui.v.song.t;
    let old = unsafe { (*h.ui.v.song.song).instr[0][0] };
    h.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(h.ui.v.pages.pending, 1);
    h.ui.press(PANEL_BTN_HOME);
    unsafe { views_set_song(&mut *h.ui.v, std::ptr::null()) };
    assert_eq!(h.ui.v.song.t, binding);
    h.f.refuse = false;
    settle(&mut h.ui);
    assert_eq!(h.ui.top(), "LIVE");
    assert_eq!(h.ui.v.pages.pending, 0);
    assert_eq!(
        unsafe { (*h.ui.v.song.song).instr[0][0] },
        old.saturating_add(16)
    );
    unsafe { views_set_song(&mut *h.ui.v, std::ptr::null()) };
    assert!(h.ui.v.song.t.is_null());
}

#[test]
fn frozen_models_refuse_new_page_edits_even_when_the_transport_guard_is_open() {
    for v in unsafe {
        [
            &song_pages_instr_view,
            &song_pages_table_view,
            &song_pages_order_view,
        ]
    } {
        let mut h = Pages::new(v);
        unsafe { (*h.model.ptr_mut()).frozen = 1 };
        h.turn(PANEL_ENC_K1, 1);
        unsafe { song_pages_poll(&mut *h.p) };
        assert_eq!(h.p.status, SONG_PAGES_SAVING as u8);
        assert_eq!(h.p.prepared, 0);
        assert_eq!(h.model.raw().pending, 0);
        assert!(h.q.edit.is_none());
        unsafe { (*h.model.ptr_mut()).frozen = 0 };
        h.finish();
        assert_eq!(h.p.status, SONG_PAGES_READY as u8);
        assert_eq!(h.p.revision, 1);
    }
}

#[test]
fn the_models_serialized_cap_maps_to_song_full_without_a_guard_approximation() {
    for v in unsafe {
        [
            &song_pages_instr_view,
            &song_pages_table_view,
            &song_pages_order_view,
        ]
    } {
        let mut h = Pages::new(v);
        let mut spec = SngSpec::new();
        spec.orders = vec![vec![(vec![206; 100], 0); 3]; 32];
        spec.patterns = vec![(vec![[GT2_REST as u8, 0, 0, 0]; 64], [0xff, 0, 0, 0]); 207];
        spec.patterns[206].0[0] = [GT2_FIRSTNOTE as u8, 1, 0, 0];
        h.model.read(&spec.encode(), 0).unwrap();
        let before = unsafe { gt2_saved_size(h.model.ptr()) };
        let trailer = unsafe { gt2_saved_trailer_size(h.model.raw().ext_used) };
        let padding = (1..SONG_EXT_BYTES - h.model.raw().ext_used - SONG_CHUNK_HEADER)
            .find(|&n| {
                before - trailer
                    + unsafe {
                        gt2_saved_trailer_size(h.model.raw().ext_used + SONG_CHUNK_HEADER + n)
                    }
                    == GT2_SAVED_MAX
            })
            .expect("padding reaches the canonical cap");
        assert_eq!(
            h.model
                .ext_set(sidou_tests::song::tag(b"CAPT"), &vec![1; padding as usize]),
            0
        );
        assert_eq!(unsafe { gt2_saved_size(h.model.ptr()) }, GT2_SAVED_MAX);
        if std::ptr::eq(v, unsafe { &song_pages_instr_view }) {
            h.turn(PANEL_ENC_PRESETS, GT2_MAX_INSTR as i16);
        } else if std::ptr::eq(v, unsafe { &song_pages_table_view }) {
            h.turn(PANEL_ENC_SELECT, GT2_TABLE_ROWS as i16);
        }
        let bytes = h.model.bytes().to_vec();
        h.turn(PANEL_ENC_K1, 1);
        unsafe { song_pages_poll(&mut *h.p) };
        assert_eq!(h.p.status, SONG_PAGES_FULL as u8);
        assert_eq!(h.p.error, GT2_ERR_SONG_FULL);
        assert_eq!(h.p.pending, 0);
        assert_eq!(h.model.bytes(), bytes);
        assert_eq!(h.model.raw().pending, 0);
        assert!(h.q.edit.is_none());
    }
}

#[test]
fn every_page_reacquires_the_canvas_after_chip_and_song_overlays_without_reentering() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut h = Song::playing(Some(Model::device()));
    open_tracker(&mut h.ui);
    for name in ["ORDER", "INSTR", "WAVE"] {
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
        assert_eq!(h.ui.big(0, 0, 2, name.len()), name);
        let expected = crate::pixels(&h.ui.g);
        let selection = (
            h.ui.v.pages.page,
            h.ui.v.pages.order_cursor,
            h.ui.v.pages.instr_cursor,
            h.ui.v.pages.table_row,
        );
        unsafe {
            screen_push(
                &mut h.ui.v.s,
                &chip_view_view,
                (&mut h.ui.v.chip as *mut chip_view).cast(),
                SCREEN_CUT,
            );
        }
        settle(&mut h.ui);
        assert_eq!(h.ui.v.song.which, SONG_VIEW_CANVAS_CHIP as u8);
        assert_eq!(
            selection,
            (
                h.ui.v.pages.page,
                h.ui.v.pages.order_cursor,
                h.ui.v.pages.instr_cursor,
                h.ui.v.pages.table_row
            )
        );
        h.ui.press(PANEL_BTN_HOME);
        settle(&mut h.ui);
        assert_eq!(crate::pixels(&h.ui.g), expected, "{name} after CHIP");
        unsafe {
            screen_push(
                &mut h.ui.v.s,
                &views_song_menu_view,
                (&mut *h.ui.v as *mut views).cast(),
                SCREEN_CUT,
            );
        }
        settle(&mut h.ui);
        assert_eq!(h.ui.v.song.which, SONG_VIEW_CANVAS_MENU as u8);
        h.ui.press(PANEL_BTN_HOME);
        settle(&mut h.ui);
        assert_eq!(crate::pixels(&h.ui.g), expected, "{name} after SONG");
    }
}

#[test]
fn page_loads_keep_the_previous_canvas_until_the_model_is_complete() {
    for v in unsafe {
        [
            &song_pages_instr_view,
            &song_pages_order_view,
            &song_pages_table_view,
        ]
    } {
        let mut h = Pages::new(v);
        h.settle();
        let expected = crate::pixels(&h.ui.g);
        let mut io = Box::<song_io>::default();
        h.ui.v.song.io = &mut *io;
        let model = h.ui.v.song.song;
        for state in [SONG_IO_STOP, SONG_IO_READ, SONG_IO_VERIFY] {
            io.state = state as u8;
            h.ui.v.song.song = std::ptr::null();
            h.ui.v.song.built = 0;
            h.settle();
            assert_eq!(crate::pixels(&h.ui.g), expected);
        }
        io.state = SONG_IO_IDLE as u8;
        h.ui.v.song.song = model;
        h.settle();
        assert_eq!(crate::pixels(&h.ui.g), expected);
    }
}

#[test]
fn nested_song_views_above_an_unnamed_save_return_to_the_original_page_and_popup() {
    use crate::song_view::{open_tracker, settle, Song};
    for (steps, title, popup) in [
        (1, "ORDER", "PATTERN"),
        (2, "INSTR", "ATTACK/DECAY"),
        (3, "WAVE", "LEFT BYTE"),
    ] {
        let mut h = Song::playing(Some(Model::device()));
        open_tracker(&mut h.ui);
        for _ in 0..steps {
            h.ui.press(PANEL_BTN_SEQ);
        }
        settle(&mut h.ui);
        h.ui.press(PANEL_BTN_REC);
        h.ui.press(SAVE_KEY_BUTTON);
        assert_eq!(h.ui.top(), "SONG");
        h.ui.press(PANEL_BTN_SEQ);
        h.ui.press(PANEL_BTN_SEQ);
        if steps == 1 {
            h.ui.press(PANEL_BTN_SEQ);
        }
        settle(&mut h.ui);
        h.ui.press(PANEL_BTN_HOME);
        h.ui.press(PANEL_BTN_HOME);
        assert_eq!(h.ui.top(), if steps == 3 { "TABLE" } else { title });
        let writes = h.f.cell_writes;
        h.ui.note_keys(1, 1);
        h.ui.note_keys(0, 0);
        assert_eq!(
            h.f.cell_writes, writes,
            "{title} must audition, not record before compose"
        );
        let mut name = [0; SCREEN_POPUP_TEXT as usize];
        let mut value = [0; SCREEN_POPUP_TEXT as usize];
        let (mut fill, mut max) = (0, 0);
        assert_eq!(
            unsafe {
                song_pages_popup(
                    &h.ui.v.pages,
                    screen_top(&h.ui.v.s),
                    PANEL_ENC_K1,
                    0,
                    name.as_mut_ptr(),
                    value.as_mut_ptr(),
                    &mut fill,
                    &mut max,
                )
            },
            1
        );
        assert_eq!(
            unsafe { CStr::from_ptr(name.as_ptr()) }.to_str(),
            Ok(popup),
            "popup identity before any callback rebuild or encoder dispatch"
        );
        settle(&mut h.ui);
        assert_eq!(
            h.ui.big(0, 0, 2, title.len()),
            title,
            "page identity after nested SEQ and HOME"
        );
        h.ui.turn(PANEL_ENC_K1, 1);
        settle(&mut h.ui);
        assert_eq!(
            unsafe { CStr::from_ptr(h.ui.v.popup.name.as_ptr()) }.to_str(),
            Ok(popup)
        );
        assert_eq!(
            h.ui.v.s.stack[h.ui.v.s.depth as usize - 1].state,
            (&mut h.ui.v.pages as *mut song_pages).cast()
        );
    }
}

#[test]
fn a_revealed_song_view_routes_keys_and_composes_its_own_page_under_midi_learn() {
    use crate::song_view::{open_tracker, settle, Song};
    for (steps, title) in [(0, "TRACKER"), (2, "INSTR"), (3, "WAVE")] {
        let mut h = Song::playing(Some(Model::device()));
        let mut learn = Box::<midi_learn>::default();
        unsafe {
            midi_learn_init(&mut *learn);
            views_learn(&mut *h.ui.v, &mut *learn);
        }
        open_tracker(&mut h.ui);
        for _ in 0..steps {
            h.ui.press(PANEL_BTN_SEQ);
        }
        settle(&mut h.ui);
        h.ui.press(PANEL_BTN_REC);
        h.ui.press(SAVE_KEY_BUTTON);
        h.ui.press(PANEL_BTN_SEQ);
        h.ui.press(PANEL_BTN_SEQ);
        settle(&mut h.ui);
        h.ui.press(PANEL_BTN_HOME);
        h.ui.press(PANEL_BTN_HOME);
        let writes = h.f.cell_writes;
        h.ui.note_keys(1, 1);
        h.ui.note_keys(0, 0);
        assert_eq!(
            h.f.cell_writes - writes,
            u32::from(steps == 0),
            "the actual revealed {title} determines recording before compose"
        );
        h.ui.learn_on(PANEL_ENC_K1 + 2);
        assert_eq!(h.ui.top(), "LEARN");
        let writes = h.f.cell_writes;
        h.ui.note_keys(2, 2);
        h.ui.note_keys(0, 0);
        assert_eq!(
            h.f.cell_writes - writes,
            u32::from(steps == 0),
            "the actual underlying {title} determines recording before compose"
        );
        settle(&mut h.ui);
        if steps != 0 {
            assert_eq!(h.ui.big(0, 0, 2, title.len()), title);
        }
        h.ui.press(PANEL_BTN_HOME);
        assert_eq!(h.ui.top(), if steps == 3 { "TABLE" } else { title });
    }
}

/// Ergonomics review E26 (M2 review 5.9): passing through ORDER keeps TRACKER's row; ORDER's
/// cursor starts on the song views' entry, and only a move to another entry starts row 0.
#[test]
fn passing_through_order_keeps_trackers_row() {
    use crate::song_view::{open_tracker, settle, Song};
    let mut h = Song::playing(Some(Model::device()));
    open_tracker(&mut h.ui);
    h.ui.v.song.row = 17;
    h.ui.press(PANEL_BTN_SEQ);
    settle(&mut h.ui);
    assert_eq!(h.ui.top(), "ORDER");
    assert_eq!(
        h.ui.v.pages.order_cursor,
        u16::from(h.ui.v.song.order_pos) + 1
    );
    assert_eq!(h.ui.v.song.row, 17, "ORDER kept the row");
    for _ in 0..4 {
        h.ui.press(PANEL_BTN_SEQ); // INSTR, TABLE, STEP, TRACKER
        settle(&mut h.ui);
    }
    assert_eq!(h.ui.top(), "TRACKER");
    assert_eq!(h.ui.v.song.row, 17, "and TRACKER has it");
}

// ---- TABLE's ARP helper ----

impl Pages {
    fn press(&mut self, id: u32) {
        self.input(SCREEN_PRESS, id, 0, 1 << id);
    }

    /// The helper's rows written: every acknowledged cell, until the chain ends.
    fn finish_arp(&mut self) {
        for _ in 0..64 {
            self.ack();
            unsafe { song_pages_poll(&mut *self.p) };
            if self.p.pending == 0 && self.p.batch_next >= self.p.batch_end {
                return;
            }
        }
        panic!("the ARP rows were not written");
    }

    fn wave_rows(&self, from: usize, n: usize) -> Vec<(u8, u8)> {
        (from..from + n)
            .map(|r| (self.model.raw().ltable[0][r], self.model.raw().rtable[0][r]))
            .collect()
    }

    fn save_arp(&mut self) {
        unsafe { song_pages_arp_save(&mut *self.p, &mut self.ui.v.s) };
    }
}

const K3: u32 = PANEL_ENC_K1 + 2;
const K4: u32 = PANEL_ENC_K1 + 3;

#[test]
fn the_arp_helper_builds_an_arpeggio_and_writes_plain_wavetable_rows() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    assert_eq!(h.p.arp, 0);
    h.press(PANEL_BTN_ARP);
    assert_eq!(h.p.arp, 1, "a blank row opens a new arpeggio");
    let d = h.p.arp_draft;
    assert_eq!((d.steps, d.speed, d.loop_, d.wave), (4, 2, 1, 0x41));
    assert_eq!(&d.off[..4], &[0, 4, 7, 12]);
    // ALGORITHM: the steps (a new one copies the last); SELECT: the step; K1 an octave, K2 a
    // semitone; K3 the speed; K4 the loop step.
    h.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(h.p.arp_draft.steps, 5);
    assert_eq!(h.p.arp_draft.off[4], 12);
    h.turn(PANEL_ENC_SELECT, 1);
    h.turn(PANEL_ENC_K2, 1);
    assert_eq!(
        h.popup(PANEL_ENC_K2, false),
        Some(("OFFSET".into(), "+5".into(), 69, 127))
    );
    h.turn(PANEL_ENC_K1, -1);
    assert_eq!(
        h.popup(PANEL_ENC_K1, false),
        Some(("OFFSET".into(), "-7".into(), 57, 127))
    );
    h.turn(K3, 1);
    assert_eq!(
        h.popup(K3, false),
        Some(("SPEED".into(), "3".into(), 2, 15))
    );
    h.turn(K4, 1);
    assert_eq!(h.popup(K4, false), Some(("LOOP".into(), "2".into(), 2, 5)));
    assert_eq!(
        h.popup(PANEL_ENC_ALGORITHM, false),
        Some(("STEPS".into(), "5".into(), 4, 7))
    );
    h.turn(K4, -5);
    assert_eq!(
        h.popup(K4, false),
        Some(("LOOP".into(), "HOLD".into(), 0, 5))
    );
    h.turn(K4, 2);
    // Nothing is written until SAVE, and then plain rows from row 01.
    assert_eq!(h.wave_rows(0, 7), vec![(0, 0); 7]);
    h.settle();
    h.save_arp();
    assert_eq!(h.ui.toast().as_deref(), Some("ARP SAVED"));
    assert_eq!(h.p.arp, 0, "SAVE closes the helper");
    h.finish_arp();
    let want = [(0x41, 0), (2, 0x79), (2, 7), (2, 12), (2, 12), (0xff, 2)];
    assert_eq!(h.wave_rows(0, 6), want);
    assert_eq!(h.wave_rows(6, 1), [(0, 0)]);
    // Each differing byte is its own acknowledged edit (blank bytes are not written).
    assert_eq!(h.q.submitted, 11);
}

#[test]
fn the_arp_helpers_presets_choose_the_waveform_the_rows_carry() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.press(PANEL_BTN_ARP);
    assert_eq!(h.p.arp_draft.wave, 0x41);
    assert_eq!(
        h.popup(PANEL_ENC_PRESETS, false),
        Some(("WAVE".into(), "PULSE".into(), 2, 7))
    );
    // The list: TRI SAW PULSE NOISE, then the combined ones; it stops at its ends.
    h.turn(PANEL_ENC_PRESETS, 1);
    assert_eq!(h.p.arp_draft.wave, 0x81);
    h.turn(PANEL_ENC_PRESETS, 1);
    assert_eq!(h.p.arp_draft.wave, 0x31);
    h.turn(PANEL_ENC_PRESETS, 9);
    assert_eq!(h.p.arp_draft.wave, 0x71);
    assert_eq!(
        h.popup(PANEL_ENC_PRESETS, false),
        Some(("WAVE".into(), "ALL 3".into(), 7, 7))
    );
    h.turn(PANEL_ENC_PRESETS, -9);
    assert_eq!(h.p.arp_draft.wave, 0x11);
    h.turn(PANEL_ENC_PRESETS, 1);
    assert_eq!(
        h.popup(PANEL_ENC_PRESETS, false),
        Some(("WAVE".into(), "SAW".into(), 1, 7))
    );
    // Nothing is written until SAVE, and then every waveform row is the chosen one.
    assert_eq!(h.wave_rows(0, 1), [(0, 0)]);
    h.settle();
    h.save_arp();
    h.finish_arp();
    let rows = h.wave_rows(0, 6);
    assert_eq!(rows[0].0, 0x21);
    assert_eq!(rows[5], (0xff, 2));
    // Reading them back opens the same waveform; a byte the list lacks is kept and shown as hex.
    h.press(PANEL_BTN_ARP);
    assert_eq!(h.p.arp_draft.wave, 0x21);
    h.turn(PANEL_ENC_PRESETS, 0);
    assert_eq!(
        h.popup(PANEL_ENC_PRESETS, false),
        Some(("WAVE".into(), "SAW".into(), 1, 7))
    );
}

#[test]
fn the_arp_helper_reads_an_arpeggio_back_and_says_when_rows_are_not_one() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.press(PANEL_BTN_ARP);
    h.turn(K3, 1);
    h.turn(K4, 1);
    h.save_arp();
    h.finish_arp();
    let written = h.wave_rows(0, 6);
    // The same rows open as the same arpeggio.
    h.press(PANEL_BTN_ARP);
    assert_eq!(h.p.arp, 1);
    let d = h.p.arp_draft;
    assert_eq!((d.steps, d.speed, d.loop_), (4, 3, 2));
    assert_eq!(&d.off[..4], &[0, 4, 7, 12]);
    assert_eq!(h.p.arp_len, 5);
    // Shortening it blanks the rows it no longer needs; HOME's close writes nothing.
    h.turn(PANEL_ENC_ALGORITHM, -2);
    assert_eq!(unsafe { song_pages_arp_close(&mut *h.p) }, 1);
    assert_eq!(h.wave_rows(0, 6), written);
    h.press(PANEL_BTN_ARP);
    h.turn(PANEL_ENC_ALGORITHM, -2);
    h.save_arp();
    h.finish_arp();
    assert_eq!(
        h.wave_rows(0, 6),
        [(0x41, 0), (2, 4), (0xff, 2), (0, 0), (0, 0), (0, 0)]
    );
    // A row that is not an arpeggio (a delay first, then not one) is told so, and nothing opens.
    h.turn(PANEL_ENC_SELECT, 1);
    h.press(PANEL_BTN_ARP);
    assert_eq!(h.p.arp, 0);
    assert_eq!(h.ui.toast().as_deref(), Some("NOT AN ARP"));
    // Another table has none.
    h.turn(PANEL_ENC_ALGORITHM, 1);
    h.press(PANEL_BTN_ARP);
    assert_eq!(h.p.arp, 0);
    assert_eq!(h.ui.toast().as_deref(), Some("WAVE TABLE ONLY"));
}

#[test]
fn the_arp_helper_never_writes_over_rows_that_are_not_blank_or_its_own() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    unsafe { (*h.model.ptr_mut()).ltable[0][4] = 0x41 }; // another program's row
    h.press(PANEL_BTN_ARP); // a new arpeggio needs 5 rows, from row 1
    h.turn(K4, -1);
    h.turn(PANEL_ENC_ALGORITHM, 2);
    h.save_arp();
    assert_eq!(h.ui.toast().as_deref(), Some("ROWS IN USE"));
    assert_eq!(h.p.arp, 1, "the helper stays for another try");
    assert_eq!(h.q.submitted, 0);
    assert_eq!(h.wave_rows(0, 6)[4], (0x41, 0));
    // Fewer steps fit before it.
    h.turn(PANEL_ENC_ALGORITHM, -3);
    h.save_arp();
    h.finish_arp();
    assert_eq!(h.wave_rows(0, 4), [(0x41, 0), (1, 4), (1, 7), (0xff, 0)]);
    assert_eq!(h.wave_rows(4, 1), [(0x41, 0)]);
}

#[test]
fn the_arp_helper_waits_for_the_edit_guard_and_keeps_its_draft_when_refused() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.press(PANEL_BTN_ARP);
    h.q.guard = SONG_EDIT_GUARD_SAVING as i32;
    h.save_arp();
    assert_eq!(h.ui.toast().as_deref(), Some("SAVING"));
    assert_eq!(h.p.arp, 1);
    assert_eq!(h.wave_rows(0, 6), vec![(0, 0); 6]);
    h.q.guard = SONG_EDIT_GUARD_OK as i32;
    h.save_arp();
    h.finish_arp();
    assert_eq!(h.wave_rows(0, 1), [(0x41, 0)]);
}

#[test]
fn the_arp_helper_shows_its_page_and_leaves_with_the_table() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.press(PANEL_BTN_ARP);
    h.turn(PANEL_ENC_SELECT, 2);
    h.settle();
    let ui = &h.ui;
    assert_eq!(ui.big(0, 0, 2, 3), "ARP");
    assert_eq!(ui.big(3, 0, 2, 5), "STEPS");
    assert_eq!(ui.big(3, 12, 2, 1), "4");
    assert_eq!(ui.big(5, 0, 2, 5), "SPEED");
    assert_eq!(ui.big(5, 12, 2, 1), "2");
    assert_eq!(ui.big(7, 0, 2, 4), "LOOP");
    assert_eq!(ui.big(7, 12, 2, 1), "1");
    // The offsets, four to a line, seven cells apart.
    assert_eq!(ui.big(11, 0, 2, 2), "+0");
    assert_eq!(ui.big(11, 7, 2, 2), "+4");
    assert_eq!(ui.big(11, 14, 2, 2), "+7");
    assert_eq!(ui.big(11, 21, 2, 3), "+12");
    assert_eq!(ui.big(17, 0, 2, 4), "WAVE");
    assert_eq!(ui.big(17, 12, 2, 5), "PULSE");
    assert_eq!(ui.big(28, 0, 2, 14), "SAVE AT ROW 01");
    // The page changing closes the draft.
    unsafe { screen_pop(&mut h.ui.v.s, SCREEN_CUT) };
    assert_eq!(h.p.arp, 0);
}

/// 1.1: while the helper is open the keys play the draft. It goes to the player on open and with
/// every change (the rows SAVE would write, from the helper's row, on the cursor's voice), and
/// HOME takes it off with nothing written; after SAVE it stays until its rows are written, then
/// goes. The header says so.
#[test]
fn the_arp_helpers_draft_is_heard_until_it_closes_or_its_rows_are_written() {
    let mut h = Pages::new(unsafe { &song_pages_table_view });
    h.press(PANEL_BTN_ARP);
    let encoded = |a: &song_pages_arp| {
        let mut l = [0u8; SONG_PAGES_ARP_ROWS as usize];
        let mut r = [0u8; SONG_PAGES_ARP_ROWS as usize];
        let n = unsafe { song_pages_arp_encode(a, 1, l.as_mut_ptr(), r.as_mut_ptr()) } as usize;
        (l[..n].to_vec(), r[..n].to_vec())
    };
    let (l, r) = encoded(&h.p.arp_draft);
    assert_eq!(h.q.drafts, vec![(0, 1, l, r)], "heard on open");
    h.settle();
    assert_eq!(h.ui.small(2, 0, 13), "HEARING DRAFT");
    h.turn(PANEL_ENC_K2, 1);
    h.turn(K3, 1);
    assert_eq!(h.q.drafts.len(), 3, "each turn heard");
    let (l, r) = encoded(&h.p.arp_draft);
    assert_eq!(h.q.drafts[2], (0, 1, l, r));
    // HOME: off, nothing written
    assert_eq!(unsafe { song_pages_arp_close(&mut *h.p) }, 1);
    assert_eq!(h.q.drafts.last().unwrap().2.len(), 0, "the draft goes");
    assert_eq!(h.wave_rows(0, 6), vec![(0, 0); 6]);
    assert_eq!(h.q.submitted, 0);
    // SAVE: heard until the rows are in
    h.press(PANEL_BTN_ARP);
    let heard = h.q.drafts.len();
    h.save_arp();
    assert_eq!(h.p.arp, 0);
    assert_eq!(h.q.drafts.len(), heard, "still heard while the rows go");
    h.finish_arp();
    unsafe { song_pages_poll(&mut *h.p) };
    assert_eq!(h.q.drafts.len(), heard + 1);
    assert_eq!(h.q.drafts.last().unwrap().2.len(), 0, "then it goes");
    assert_eq!(h.p.arp_heard, 0);
    assert_eq!(h.wave_rows(0, 1), [(0x41, 0)]);
}

// ---- the batch writer (1.5: the ARP helper's and LAB's send) ----

impl Pages {
    fn table_rows(&self, table: usize, from: usize, n: usize) -> Vec<(u8, u8)> {
        (from..from + n)
            .map(|r| {
                (
                    self.model.raw().ltable[table][r],
                    self.model.raw().rtable[table][r],
                )
            })
            .collect()
    }

    fn batch_rows(
        &mut self,
        table: u32,
        first: u32,
        rows: &[(u8, u8)],
        had: u32,
    ) -> Option<String> {
        let l: Vec<u8> = rows.iter().map(|r| r.0).collect();
        let r: Vec<u8> = rows.iter().map(|r| r.1).collect();
        let why = unsafe {
            song_pages_batch_rows(
                &mut *self.p,
                table,
                first,
                rows.len() as u32,
                had,
                l.as_ptr(),
                r.as_ptr(),
            )
        };
        (!why.is_null()).then(|| unsafe { CStr::from_ptr(why) }.to_str().unwrap().to_owned())
    }
}

/// A batch of an instrument and rows of two tables: written in the order added, a differing byte
/// per acknowledged edit (the bytes the song has already are skipped), the rows first so that the
/// instrument's pointers never name rows not written yet.
#[test]
fn the_batch_writer_writes_rows_then_the_instrument_a_differing_byte_at_a_time() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    unsafe { (*h.model.ptr_mut()).rtable[1][3] = 0x40 }; // the pulse row's right byte already there
    assert_eq!(unsafe { song_pages_batch_begin(&mut *h.p) }, 1);
    assert_eq!(h.batch_rows(GT2_WTBL, 0, &[(0x41, 0), (0xff, 0)], 0), None);
    assert_eq!(
        h.batch_rows(GT2_PTBL, 3, &[(0x88, 0x40), (0xff, 0)], 1),
        None
    );
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x19, 0xa9, 1, 4, 0, 0, 0, 2, 0x41]);
    ins[9..13].copy_from_slice(b"LAB1");
    assert!(unsafe { song_pages_batch_instr(&mut *h.p, 5, ins.as_ptr()) }.is_null());
    assert_eq!(unsafe { song_pages_batch_go(&mut *h.p) }, 1);
    assert_eq!(unsafe { song_pages_batch_busy(&*h.p) }, 1);
    // The first byte is out; the rest wait for its acknowledgement.
    assert_eq!(h.q.submitted, 1);
    assert_eq!(h.p.request.kind, SONG_PAGES_EDIT_TABLE);
    h.finish_arp();
    assert_eq!(unsafe { song_pages_batch_busy(&*h.p) }, 0);
    assert_eq!(h.table_rows(0, 0, 2), [(0x41, 0), (0xff, 0)]);
    assert_eq!(h.table_rows(1, 3, 2), [(0x88, 0x40), (0xff, 0)]);
    assert_eq!(h.model.raw().instr[4], ins);
    // 0x41, 0xff; 0x88, 0xff (0x40 there already); the instrument's 10 bytes that are not 0.
    assert_eq!(h.q.submitted, 2 + 2 + 10);
}

/// TABLE FULL past row 255, ROWS IN USE over another program's rows, and the rows a shorter
/// program no longer needs blanked; nothing is added on a refusal.
#[test]
fn the_batch_writer_refuses_full_tables_and_rows_in_use_and_blanks_what_is_left() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    for r in 10..14 {
        unsafe {
            (*h.model.ptr_mut()).ltable[2][r] = 0x90 + r as u8;
            (*h.model.ptr_mut()).rtable[2][r] = 1;
        }
    }
    unsafe { song_pages_batch_begin(&mut *h.p) };
    let three = [(0x90, 0xf1), (0, 0x40), (0xff, 0)];
    assert_eq!(
        h.batch_rows(GT2_FTBL, 253, &three, 0).as_deref(),
        Some("TABLE FULL")
    );
    assert_eq!(
        h.batch_rows(GT2_FTBL, 8, &three, 0).as_deref(),
        Some("ROWS IN USE")
    );
    // its own four rows from 10: three written, the fourth blanked
    assert_eq!(h.batch_rows(GT2_FTBL, 10, &three, 4), None);
    assert!(unsafe { song_pages_batch_blank(&mut *h.p, GT2_FTBL, 20, 1) }.is_null());
    assert_eq!(unsafe { song_pages_batch_go(&mut *h.p) }, 1);
    h.finish_arp();
    assert_eq!(
        h.table_rows(2, 8, 6),
        [(0, 0), (0, 0), (0x90, 0xf1), (0, 0x40), (0xff, 0), (0, 0)]
    );
    // Nothing is begun again while one is written.
    unsafe { song_pages_batch_begin(&mut *h.p) };
    assert_eq!(h.batch_rows(GT2_WTBL, 0, &[(0x21, 0), (0xff, 0)], 0), None);
    unsafe { song_pages_batch_go(&mut *h.p) };
    assert_eq!(unsafe { song_pages_batch_begin(&mut *h.p) }, 0);
    h.finish_arp();
    assert_eq!(unsafe { song_pages_batch_begin(&mut *h.p) }, 1);
}

/// A write the guard refuses ends the batch: the bytes after it are not written, and the status
/// says why.
#[test]
fn the_batch_writer_stops_at_a_guard_refusal() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    unsafe { song_pages_batch_begin(&mut *h.p) };
    h.batch_rows(GT2_WTBL, 0, &[(0x41, 0), (0x21, 0), (0xff, 0)], 0);
    unsafe { song_pages_batch_go(&mut *h.p) };
    h.ack();
    h.q.guard = SONG_EDIT_GUARD_FULL as i32;
    unsafe { song_pages_poll(&mut *h.p) };
    assert_eq!(h.p.status as u32, SONG_PAGES_FULL);
    assert_eq!(unsafe { song_pages_batch_busy(&*h.p) }, 0);
    for _ in 0..8 {
        h.ack();
        unsafe { song_pages_poll(&mut *h.p) };
    }
    assert_eq!(h.table_rows(0, 0, 3), [(0x41, 0), (0, 0), (0, 0)]);
    // Refused at once: go says so.
    h.q.guard = SONG_EDIT_GUARD_SAVING as i32;
    unsafe { song_pages_batch_begin(&mut *h.p) };
    assert_eq!(h.batch_rows(GT2_WTBL, 0, &[(0x11, 0)], 1), None);
    assert_eq!(unsafe { song_pages_batch_go(&mut *h.p) }, 0);
    assert_eq!(h.p.status as u32, SONG_PAGES_SAVING);
}

// ---- 1.5: LAB's send (app/lab_song.h) through the batch writer ----

use crate::lab_sound::Sound;

impl Pages {
    fn send(&mut self, s: &Sound, instr: u32) -> Result<(u32, u32), String> {
        let (mut i, mut notes) = (instr, 0);
        let why = unsafe {
            song_pages_lab_send(
                &mut *self.p,
                s.as_ptr(),
                c"LAB 03".as_ptr(),
                0,
                &mut i,
                &mut notes,
            )
        };
        if why.is_null() {
            Ok((i, notes))
        } else {
            Err(unsafe { CStr::from_ptr(why) }.to_str().unwrap().to_owned())
        }
    }

    fn instr(&self, i: usize) -> [u8; 25] {
        self.model.raw().instr[i - 1]
    }
}

fn modulated() -> Sound {
    let mut s = crate::lab_song::plain();
    s[LAB_SOUND_PULSE as usize..][..2].copy_from_slice(&2048u16.to_le_bytes());
    s[LAB_SOUND_PWM_DEPTH as usize..][..2].copy_from_slice(&512u16.to_le_bytes());
    s[LAB_SOUND_PWM_RATE as usize..][..2].copy_from_slice(&655u16.to_le_bytes());
    s[LAB_SOUND_FILTER_VOICES as usize] = 7;
    s[LAB_SOUND_VIBRATO_RATE as usize] = 60;
    s[LAB_SOUND_VIBRATO_DEPTH as usize] = 64;
    s
}

/// A send takes the lowest free instrument and puts each program after its table's last row
/// (a speed row the table has already is shared), the filter on the song views' voice; it is
/// written a cell at a time while the song plays, rows first, and the chunk keeps the sound.
#[test]
fn a_send_takes_the_next_free_instrument_and_the_rows_after_each_tables_last() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    unsafe {
        let m = &mut *h.model.ptr_mut();
        m.instr[0][0] = 0x11; // instrument 1 in use
        for r in 0..3 {
            m.ltable[0][r] = 0x21; // three wave rows
        }
        m.ltable[1][0] = 0x88; // a pulse row
        m.ltable[3][0] = 0x81; // the speed row the vibrato needs
        m.rtable[3][0] = 2;
    }
    h.ui.v.song.voice = 1;
    let s = modulated();
    let (i, notes) = h.send(&s, 0).unwrap();
    assert_eq!(i, 2);
    assert_eq!(notes, LAB_SONG_PWM | LAB_SONG_FILTER | LAB_SONG_VIB);
    let want = crate::lab_song::encode(&s, 1, [4, 2, 1, 1]);
    assert_eq!(
        h.instr(2),
        [0; 25],
        "nothing until the edits are acknowledged"
    );
    h.finish_arp();
    assert_eq!(h.instr(2), want.instr);
    assert_eq!(
        (want.instr[2], want.instr[3], want.instr[4], want.instr[5]),
        (4, 2, 1, 1)
    );
    for t in 0..3 {
        let at = [3, 1, 0][t];
        assert_eq!(
            h.table_rows(t, at, want.rows[t] as usize),
            crate::lab_song::rows(&want, t),
            "table {t}"
        );
    }
    assert_eq!(
        h.table_rows(3, 0, 2),
        [(0x81, 2), (0, 0)],
        "the speed row shared"
    );
    assert_eq!(want.right[2][0] & 7, 2, "the filter on voice 2");
    // the chunk keeps the sound, its CRC that of the bytes as the queue wrote them
    let (mut rows, mut kept) = ([0u8; 4], [0u8; LAB_SOUND_BYTES as usize]);
    assert_eq!(
        unsafe { lab_song_kept(h.model.ptr(), 2, rows.as_mut_ptr(), kept.as_mut_ptr()) },
        1
    );
    assert_eq!((kept, rows), (s, want.rows));
    // The next goes to the next free one, after these.
    let (i, _) = h.send(&crate::lab_song::plain(), 0).unwrap();
    assert_eq!(i, 3);
    h.finish_arp();
    assert_eq!(h.instr(3)[2], 4 + want.rows[0]);
}

/// EDIT + send over an unedited sent instrument: its programs where they were, the rows they no
/// longer need blank; over an instrument edited since (or not sent), the programs go after the
/// tables' last rows and its old rows stay.
#[test]
fn a_send_over_a_sent_instrument_writes_in_its_rows_and_blanks_the_rest() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    let (i, _) = h.send(&modulated(), 0).unwrap();
    h.finish_arp();
    let old = h.instr(i as usize);
    let pulse = old[3] as usize;
    assert_eq!(
        h.table_rows(1, pulse - 1, 6)[4].0,
        0xff,
        "a five-row pulse loop"
    );
    let plain = crate::lab_song::plain();
    assert_eq!(h.send(&plain, i).unwrap().0, i);
    h.finish_arp();
    let new = h.instr(i as usize);
    assert_eq!((new[2], new[3]), (old[2], old[3]), "in place");
    assert_eq!(
        h.table_rows(1, pulse - 1, 5),
        [(0x88, 0x12), (0xff, 0), (0, 0), (0, 0), (0, 0)]
    );
    assert_eq!((new[4], new[5]), (0, 0), "no filter, no vibrato now");
    assert_eq!(
        h.table_rows(2, 0, 3),
        [(0, 0); 3],
        "the filter's rows blank"
    );
    // Edited since: no longer its own rows.
    unsafe { (*h.model.ptr_mut()).instr[i as usize - 1][0] = 0x55 };
    h.send(&modulated(), i).unwrap();
    h.finish_arp();
    assert_eq!(
        h.instr(i as usize)[3],
        3,
        "after the pulse table's last row"
    );
    assert_eq!(
        h.table_rows(1, pulse - 1, 2),
        [(0x88, 0x12), (0xff, 0)],
        "left as it was"
    );
}

#[test]
fn a_send_says_why_it_cannot() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    let s = crate::lab_song::plain();
    h.send(&s, 0).unwrap();
    assert_eq!(h.send(&s, 0).unwrap_err(), "WAITING");
    h.finish_arp();
    let submitted = h.q.submitted;
    for r in 0..255 {
        unsafe { (*h.model.ptr_mut()).ltable[0][r] = 0x41 };
    }
    assert_eq!(h.send(&s, 0).unwrap_err(), "TABLE FULL");
    for i in 0..63 {
        unsafe { (*h.model.ptr_mut()).instr[i][7] = 2 };
    }
    assert_eq!(h.send(&s, 0).unwrap_err(), "NO FREE INSTR");
    assert_eq!(h.q.submitted, submitted, "nothing written on a refusal");
}

/// REC in LAB sends the sound and says where; EDIT + REC asks before it replaces INSTR's
/// instrument, and replaces it on a second EDIT + REC. With the arpeggio on, TABLE opens on its
/// chord with the ARP helper. ENV on INSTR pulls the instrument into LAB.
#[test]
fn rec_in_lab_sends_and_edit_rec_asks_before_it_replaces() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    h.ui.press(PANEL_BTN_ENV);
    assert_eq!(h.ui.toast().as_deref(), Some("EMPTY INSTRUMENT"));
    assert_eq!(h.ui.top(), "INSTR");
    // to LAB from the menu, a sound with a filter and the arpeggio
    unsafe {
        let mut s = crate::lab_song::plain();
        s[LAB_SOUND_FILTER_VOICES as usize] = 7;
        live_sound_apply(&mut *h.ui.rig.live, s.as_ptr());
    }
    unsafe {
        screen_push(
            &mut h.ui.v.s,
            &lab_view_view,
            (&mut h.ui.v.lab as *mut lab_view).cast(),
            SCREEN_CUT,
        )
    };
    h.ui.press(PANEL_BTN_REC);
    let toast = h.ui.toast().unwrap();
    assert!(toast.starts_with("SENT AS I01 "), "{toast}");
    for _ in 0..64 {
        h.ack();
        h.ui.frame();
    }
    assert_eq!(h.model.raw().instr[0][0], 0x29);
    let mut hint = [0 as std::ffi::c_char; 31];
    let text = unsafe { lab_view_hint_text(&h.ui.v.lab, h.ui.now, hint.as_mut_ptr()) };
    assert_eq!(
        unsafe { CStr::from_ptr(text) }.to_str().unwrap(),
        "APPROX: FILTER"
    );
    assert_eq!(h.ui.v.song.instr, 1, "INSTR shows it");
    // EDIT + REC: a question, then the replace
    h.ui.rig.live.attack_decay = 0x48;
    h.ui.hold(PANEL_BTN_EDIT, true);
    h.ui.press(PANEL_BTN_REC);
    assert_eq!(h.ui.toast().as_deref(), Some("REPLACE I01? EDIT+REC"));
    h.ui.press(PANEL_BTN_REC);
    h.ui.hold(PANEL_BTN_EDIT, false);
    assert!(h.ui.toast().unwrap().starts_with("SENT AS I01"));
    for _ in 0..64 {
        h.ack();
        h.ui.frame();
    }
    assert_eq!(h.model.raw().instr[0][0], 0x48);
    // the arpeggio: TABLE's helper on its chord
    unsafe { live_arp_set(&mut *h.ui.rig.live, LIVE_ARP_ON, 1) };
    h.ui.press(PANEL_BTN_REC);
    assert_eq!(h.ui.top(), "TABLE");
    assert_eq!(h.ui.v.pages.arp, 1);
    assert_eq!(h.ui.v.pages.arp_draft.steps, 4);
}

/// ENV, LFO or FX on INSTR pull its instrument into LAB at their page: a sent one exactly, as
/// one undo step; a kit drum decoded, the toast saying what was approximated.
#[test]
fn env_lfo_and_fx_on_instr_pull_the_instrument_into_lab() {
    let mut h = Pages::new(unsafe { &song_pages_instr_view });
    let s = modulated();
    h.send(&s, 0).unwrap();
    h.finish_arp();
    let sound = |h: &Pages| {
        let mut now = [0u8; LAB_SOUND_BYTES as usize];
        unsafe { live_sound_get(&*h.ui.rig.live, now.as_mut_ptr()) };
        now
    };
    let before = sound(&h);
    assert_ne!(before, s);
    h.ui.v.song.instr = 1;
    h.ui.press(PANEL_BTN_ENV);
    assert_eq!(h.ui.top(), "LAB");
    assert_eq!(u32::from(h.ui.v.lab.page), LAB_PAGE_ENVELOPE);
    assert_eq!(h.ui.toast().as_deref(), Some("PULLED I01"));
    assert_eq!(sound(&h), s, "exactly the sound sent");
    // one undo step
    h.ui.down(PANEL_BTN_EDIT);
    h.ui.press(PANEL_BTN_LFO);
    h.ui.up(PANEL_BTN_EDIT);
    assert_eq!(sound(&h), before);
    // a kit drum, from FX
    unsafe {
        screen_pop(&mut h.ui.v.s, SCREEN_CUT);
        let m = &mut *h.model.ptr_mut();
        m.instr[1] = std::mem::transmute::<gt2_kit_drum, [u8; 25]>(gt2_kit_instr[1]);
        let wave = (&raw const gt2_kit_wave).read();
        m.ltable[0][100..][..wave[0].len()].copy_from_slice(&wave[0]);
        m.rtable[0][100..][..wave[1].len()].copy_from_slice(&wave[1]);
        m.instr[1][2] = 100 + 9; // the snare's rows, moved
    }
    h.ui.v.song.instr = 2;
    h.ui.press(PANEL_BTN_FX);
    assert_eq!(u32::from(h.ui.v.lab.page), LAB_PAGE_FX);
    let toast = h.ui.toast().unwrap();
    assert!(
        toast.starts_with("I02 ~") && toast.contains("DRUM"),
        "{toast}"
    );
}
