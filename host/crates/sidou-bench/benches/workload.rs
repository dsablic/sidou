// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The shared workload on the host: {1, 2} chips x {6581, 8580}. Throughput counts samples
//! times chips, so criterion reports the rate per chip. workload_begin (sid_init, which
//! fills the cutoff table) is setup and is not timed.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use sidou_sys::workload::Workload;
use sidou_sys::{SID_MODEL_6581, SID_MODEL_8580};

fn workload(c: &mut Criterion) {
    let w = Workload::lock();
    let mut out = vec![0i16; w.sample_count()];
    let mut group = c.benchmark_group("workload");
    for chips in [1u32, 2] {
        for (model, name) in [(SID_MODEL_6581, "6581"), (SID_MODEL_8580, "8580")] {
            group.throughput(Throughput::Elements(out.len() as u64 * u64::from(chips)));
            group.bench_function(BenchmarkId::new(name, format!("{chips}chip")), |b| {
                b.iter_batched(
                    || w.begin(chips, model),
                    |()| {
                        w.render_samples(&mut out);
                        black_box(&out);
                    },
                    BatchSize::PerIteration,
                )
            });
        }
    }
    group.finish();
}

criterion_group!(benches, workload);
criterion_main!(benches);
