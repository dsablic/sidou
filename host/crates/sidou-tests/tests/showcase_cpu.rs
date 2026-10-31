// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::cpu::Access;
use gt2_ref::trace::cross_check;
use gt2_ref::Options;
use sidou_tests::showcase;

#[test]
fn showcase_songs_match_on_sidou_s_6502() {
    let o = Options::default();
    let mut total = 0;
    for s in showcase::all() {
        let c = cross_check(
            &s.sng(),
            0,
            3000,
            &o,
            &[Access::HookAll, Access::Device, Access::HookInline],
        )
        .unwrap_or_else(|e| panic!("{}: {e}", s.name));
        assert!(c.writes > 0 && c.timed_calls == c.calls, "{}", s.name);
        eprintln!("{}: {} calls, {} writes", s.name, c.calls, c.writes);
        total += 1;
    }
    assert!(total > 0);
}
