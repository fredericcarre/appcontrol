//! FSM micro-benchmarks.
//!
//! Why this matters: every health-check result coming back from an agent runs
//! through `is_valid_transition` and `next_state_from_check` at least once
//! (and usually twice — once on the gateway, once on the backend). With
//! thousands of components per backend reporting at 30s intervals, these two
//! pure-CPU functions sit on the critical path of every state update. The
//! goal of these benches is to show the FSM is *cheap* — i.e. the FSM itself
//! is never the bottleneck, only the database write afterwards.
//!
//! Two benchmark groups:
//!  - `is_valid_transition`: every (from, to) pair across the 8 states (64 cases)
//!    measured as one group at a fixed throughput target.
//!  - `next_state_from_check`: every (state, exit_code in {0,1,2}) pair
//!    (8 × 3 = 24 cases), each measured individually.

use appcontrol_common::fsm::{is_valid_transition, next_state_from_check};
use appcontrol_common::types::ComponentState;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

const ALL_STATES: [ComponentState; 8] = [
    ComponentState::Unknown,
    ComponentState::Running,
    ComponentState::Degraded,
    ComponentState::Failed,
    ComponentState::Stopped,
    ComponentState::Starting,
    ComponentState::Stopping,
    ComponentState::Unreachable,
];

const EXIT_CODES: [i32; 3] = [0, 1, 2];

fn bench_is_valid_transition(c: &mut Criterion) {
    let mut group = c.benchmark_group("fsm::is_valid_transition");
    // Each iteration walks the full 8×8 = 64-entry transition matrix so that
    // the measurement is independent of branch-prediction luck on any single
    // pair. Throughput is reported in matrix walks per second.
    group.throughput(Throughput::Elements(64));
    group.bench_function("full_matrix_8x8", |b| {
        b.iter(|| {
            let mut acc = 0u32;
            for from in ALL_STATES {
                for to in ALL_STATES {
                    if is_valid_transition(black_box(from), black_box(to)) {
                        acc += 1;
                    }
                }
            }
            black_box(acc)
        });
    });

    // Also report per-call throughput for the most common transition the
    // sequencer actually exercises: Running → Running (no-op check OK path).
    group.throughput(Throughput::Elements(1));
    group.bench_function("hot_path_running_running", |b| {
        b.iter(|| {
            is_valid_transition(
                black_box(ComponentState::Running),
                black_box(ComponentState::Failed),
            )
        });
    });

    group.finish();
}

fn bench_next_state_from_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("fsm::next_state_from_check");
    // 8 states × 3 exit codes = 24 entries walked per iteration.
    group.throughput(Throughput::Elements(24));
    group.bench_function("full_grid_8x3", |b| {
        b.iter(|| {
            let mut hit = 0u32;
            for state in ALL_STATES {
                for ec in EXIT_CODES {
                    if next_state_from_check(black_box(state), black_box(ec)).is_some() {
                        hit += 1;
                    }
                }
            }
            black_box(hit)
        });
    });

    // The hot path in production: Running, exit 0 (process healthy, no change).
    // This is the path every successful health check takes.
    group.throughput(Throughput::Elements(1));
    group.bench_function("hot_path_running_exit0", |b| {
        b.iter(|| next_state_from_check(black_box(ComponentState::Running), black_box(0)));
    });

    // Second hot path: Running, exit 1 — escalates to Failed. This is the
    // path that drives an actual state_transitions INSERT in the backend.
    group.bench_function("hot_path_running_exit1", |b| {
        b.iter(|| next_state_from_check(black_box(ComponentState::Running), black_box(1)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_is_valid_transition,
    bench_next_state_from_check
);
criterion_main!(benches);
