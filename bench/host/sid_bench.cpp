// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <benchmark/benchmark.h>

#include <vector>

extern "C" {
#include "workload.h"
}

static void BM_Workload(benchmark::State& state) {
    const auto chips = (uint32_t)state.range(0);
    const auto model = (sid_model)state.range(1);
    std::vector<int16_t> out(workload_sample_count());
    for (auto _ : state) {
        state.PauseTiming();
        workload_begin(chips, model);
        state.ResumeTiming();
        workload_render_samples(out.data(), (uint32_t)out.size());
        benchmark::DoNotOptimize(out.data());
    }
    state.SetItemsProcessed(state.iterations() * (int64_t)out.size());
    state.counters["chips"] = (double)chips;
}
BENCHMARK(BM_Workload)
    ->ArgsProduct({{1, 2}, {SID_MODEL_6581, SID_MODEL_8580}})
    ->ArgNames({"chips", "model"})
    ->Unit(benchmark::kMicrosecond);
