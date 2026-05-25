# Security Roadmap

Living tracker for security findings that are not yet resolved on the
default branch. Every entry is visible in `cargo audit` and in the
DORA quality attestation report. **No advisory is suppressed via
`[advisories.ignore]` in `deny.toml`** — by policy, suppressing
findings is forbidden; this file is the public record of every open
item and the path to clear it.

## Open advisories

### RUSTSEC-2026-0097 — `rand 0.8` unsoundness with custom logger

**Status:** open · **Severity:** unsound (not exploitable in current
configuration) · **Source paths:** four independent transitive
dependencies pull `rand 0.8` into the lockfile.

```
rand 0.8.5
├── sqlx-postgres 0.8.6
├── sqlx-mysql 0.8.6     (via num-bigint-dig → rsa)
└── axum 0.7.9           (via tokio-tungstenite 0.24 → tungstenite 0.24)
```

**Why we cannot clear this today:**

- The `sqlx` family (0.8.6 is the latest release as of 2026-05) has
  not yet shipped a release that drops the `rand 0.8` dep. Upstream
  tracking issue: https://github.com/launchbadge/sqlx/issues (search
  for "rand 0.8"). Until a new sqlx release lands, the backend
  cannot avoid this path.
- `rsa` (pulled by `sqlx-mysql` via `num-bigint-dig`) is in the
  same situation — it is a foundational crate where the rand
  migration is non-trivial.
- `axum 0.7.9` pulls `tokio-tungstenite 0.24` transitively. Bumping
  AppControl's direct `tokio-tungstenite` dep to 0.29 (done) does
  not eliminate axum's transitive 0.24. The fix is to bump axum to
  0.8.x, which is a breaking major release across all our HTTP
  handlers — a tracked refactor for a future sprint.

**Exploitability assessment:** the advisory only triggers if a
custom logger calls `rand::rng()` from a non-async context. AppControl
does not install a custom logger that meets this condition: we use
`tracing-subscriber` with the default sink, and no code path inside
AppControl calls `rand` via the unsound code path. The finding is
therefore **informational** for our deployment, but we leave it
visible to make the dependency state honest.

**Path to clear:**

1. Watch for the next `sqlx` release that bumps `rand` and bump on
   ship.
2. Plan the `axum 0.7 → 0.8` migration (estimate: 1-2 weeks of
   refactor across backend handlers).
3. Once both upstreams ship, the advisory clears with a lockfile
   bump.

---

## Resolved advisories

Kept here as a public record of what shipped on the default branch:

| ID | Crate | Cleared by | When |
|---|---|---|---|
| RUSTSEC-2025-0134 | rustls-pemfile | Migrated to `rustls::pki_types::pem::PemObject` (commit `ee224b5`) | 2026-05 |
| RUSTSEC-2025-0057 | fxhash | Replaced sled with redb (commit `16c5a3f`) | 2026-05 |
| RUSTSEC-2024-0384 | instant | Replaced sled with redb (commit `16c5a3f`) | 2026-05 |
| (npm GHSA chain on axios 1.0–1.15) | axios | Bumped lockfile to 1.16.0 (commit included in PR #118) | 2026-05 |

---

## Policy

- **No `ignore = [...]` in `deny.toml`.** Every advisory must
  appear in the quality attestation report. Hiding a finding is
  worse than acknowledging it openly with a plan.
- **No `unmaintained` is downgraded to a scope below `"all"`** —
  the scope is `all` so warnings surface even on deep transitive
  deps, which is exactly the case for the upstream-blocked rand
  paths above.
- **License compliance is allow-listed explicitly.** A new license
  that lands via a dependency bump must be added to `deny.toml`
  with a comment justifying the decision; it does not enter
  silently.
- **Workspace crates are marked `publish = false`** + cargo-deny is
  configured with `[licenses.private] ignore = true`. Proprietary
  internal crates do not need to fake a per-crate SPDX license; the
  `Proprietary` workspace-level field is documentation only. Every
  third-party dep is still checked against the explicit allow list.

## Report ↔ this file

The quality attestation report (`release-quality-report.yaml` →
`QUALITY-REPORT.md`) shows the live counts. When the count is
non-zero, the corresponding entry in this file gives the why and
the planned remediation. The two should stay in sync; if you add a
finding to one, add it to the other in the same PR.
