//! Permission-resolution benchmarks.
//!
//! Why this matters: every authenticated API request resolves
//! `effective_permission(user_id, app_id)` to decide whether to 403 or
//! proceed. The function is two SELECTs against `app_permissions_users` and
//! `app_permissions_teams` (joined through `team_members`). On a healthy
//! install both queries return zero or a handful of rows per call, so the
//! cost is dominated by the round trip + planner — not the data set.
//!
//! These benches deliberately pick the *medium* and *large* fixtures so we
//! can publish numbers that bracket what a real install looks like, rather
//! than the trivial 10-user/10-app case.
//!
//! Three scenarios:
//!  - small  : 10 users × 10 teams × 10 apps × 100 grants
//!  - medium : 1000 users × 100 teams × 1000 apps × 10 000 grants
//!  - large  : 10 000 users × 1000 teams × 10 000 apps × 100 000 grants
//!
//! All run against an in-memory SQLite (the function under test is compiled
//! with the `sqlite` feature). On PostgreSQL the same query plans are
//! index-backed and faster — these numbers are a conservative lower bound.

use appcontrol_backend::core::permissions::effective_permission;
use appcontrol_benchmarks::permission_support::{build_seeded_db, SeedSpec, SeededDb};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::runtime::Runtime;

struct Fixture {
    rt: Runtime,
    db: SeededDb,
}

impl Fixture {
    fn new(spec: SeedSpec) -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let db = rt.block_on(build_seeded_db(&spec));
        Self { rt, db }
    }
}

fn bench_effective_permission(c: &mut Criterion) {
    let mut group = c.benchmark_group("permissions::effective_permission");
    group.throughput(Throughput::Elements(1));

    let scenarios = [
        (
            "small_10x10x10x100",
            SeedSpec {
                users: 10,
                teams: 10,
                apps: 10,
                grants: 100,
            },
        ),
        (
            "medium_1k_100_1k_10k",
            SeedSpec {
                users: 1_000,
                teams: 100,
                apps: 1_000,
                grants: 10_000,
            },
        ),
        (
            "large_10k_1k_10k_100k",
            SeedSpec {
                users: 10_000,
                teams: 1_000,
                apps: 10_000,
                grants: 100_000,
            },
        ),
    ];

    for (name, spec) in scenarios {
        let fixture = Fixture::new(spec);
        // Pick a stable (user, app) pair. Picking index 0 means the same
        // (user, app) is queried every iteration — this is what production
        // looks like in steady state (the same authenticated user keeps
        // hitting the same set of apps in a session).
        let user_id = fixture.db.user_ids[0];
        let app_id = fixture.db.app_ids[0];
        let pool = fixture.db.pool.clone();

        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &(pool, user_id, app_id),
            |b, (pool, user, app)| {
                b.to_async(&fixture.rt).iter(|| async {
                    let lvl = effective_permission(
                        black_box(pool),
                        black_box(*user),
                        black_box(*app),
                        false,
                    )
                    .await;
                    black_box(lvl)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_effective_permission);
criterion_main!(benches);
