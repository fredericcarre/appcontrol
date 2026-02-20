# COVERAGE.md - Test Coverage Strategy

## Objectives
- **Backend (Rust):** ≥80% line coverage on `crates/backend/src/core/` (FSM, DAG, permissions, switchover, diagnostic)
- **Backend API:** ≥70% line coverage on `crates/backend/src/api/` (every endpoint has at least 1 happy path + 1 error path test)
- **Agent:** ≥75% line coverage on `crates/agent/src/` (executor, scheduler, buffer are critical)
- **Common:** ≥90% line coverage (pure logic, easy to test exhaustively)
- **Frontend:** ≥60% coverage on components with logic (hooks, stores, permission checks)
- **E2E:** 9 scenario tests covering all major user journeys

## Tools

### Rust: cargo-llvm-cov
```bash
# Install
cargo install cargo-llvm-cov

# Run with coverage
cargo llvm-cov --workspace --html --output-dir coverage/

# Run with minimum threshold (CI gate)
cargo llvm-cov --workspace --fail-under-lines 70

# Per-crate coverage
cargo llvm-cov --package appcontrol-common --fail-under-lines 90
cargo llvm-cov --package appcontrol-backend --fail-under-lines 75
cargo llvm-cov --package appcontrol-agent --fail-under-lines 75
```

### Frontend: vitest + v8 coverage
```bash
# In frontend/vite.config.ts, add:
# test: { coverage: { provider: 'v8', reporter: ['text', 'html', 'lcov'], thresholds: { lines: 60 } } }

cd frontend && npm run test -- --coverage
```

## CI Integration
Add to `.github/workflows/ci.yaml`:
```yaml
- name: Rust coverage
  run: |
    cargo install cargo-llvm-cov
    cargo llvm-cov --workspace --lcov --output-path lcov.info
    cargo llvm-cov --package appcontrol-common --fail-under-lines 90
    cargo llvm-cov --package appcontrol-backend --fail-under-lines 70

- name: Frontend coverage
  run: |
    cd frontend && npm run test -- --coverage --passWithNoTests

- name: Upload coverage
  uses: codecov/codecov-action@v4
  with:
    files: lcov.info,frontend/coverage/lcov.info
```

## Coverage Targets by Module

| Module | Target | Rationale |
|--------|--------|-----------|
| `common/fsm.rs` | 100% | Pure logic, all transitions testable |
| `common/types.rs` | 95% | Enums, serialization |
| `common/protocol.rs` | 90% | Serialization roundtrips |
| `backend/core/fsm.rs` | 95% | Critical: drives all state changes |
| `backend/core/dag.rs` | 95% | Critical: sequencing correctness |
| `backend/core/sequencer.rs` | 85% | Complex async logic |
| `backend/core/branch.rs` | 90% | Graph traversal, must be correct |
| `backend/core/permissions.rs` | 95% | Security: every path must be tested |
| `backend/core/switchover.rs` | 80% | 6 phases × success/failure |
| `backend/core/diagnostic.rs` | 95% | 8 matrix combinations |
| `backend/core/rebuild.rs` | 80% | DAG + protection + bastion |
| `backend/api/*.rs` | 70% | Each endpoint: happy + auth fail + not found |
| `agent/executor.rs` | 80% | Process detachment is critical |
| `agent/scheduler.rs` | 80% | Timing accuracy |
| `agent/buffer.rs` | 85% | Offline reliability |
| `frontend/hooks/` | 70% | Logic-heavy hooks |
| `frontend/stores/` | 70% | State management |
| `frontend/components/maps/` | 50% | UI rendering, harder to test |

## Test Pyramid

```
        /\
       /  \   E2E Tests (9 scenarios)
      /    \  Full stack, real DB, real WebSocket
     /------\
    /        \ Integration Tests (~50 tests)
   /          \ Backend API + DB, Agent + checks
  /------------\
 /              \ Unit Tests (~200+ tests)
/                \ FSM, DAG, permissions, serialization, pure logic
```

### Unit Tests (per crate)
- `common`: FSM transitions, type serialization, permission ordering
- `backend/core`: DAG sort, branch detection, permission resolution, diagnostic matrix
- `agent`: check dedup, buffer FIFO, native command parsing

### Integration Tests (backend + DB)
- API CRUD with real PostgreSQL
- Permission enforcement on every endpoint
- State transitions written correctly
- Partition creation for check_events
- Config versioning on every change

### E2E Tests (full stack)
1. `test_full_start_stop` — DAG sequencing, suspension on failure
2. `test_branch_restart` — Error branch detection, selective restart
3. `test_switchover` — 6-phase DR, rollback, RTO measurement
4. `test_diagnostic_rebuild` — 3-level diagnosis, protection, DAG rebuild
5. `test_custom_commands` — Execution, confirmation, RBAC
6. `test_permissions_sharing` — 6 levels, teams, share links, expiry
7. `test_audit_trail` — Completeness, append-only, config versioning
8. `test_agent_offline` — Disconnect, buffer, replay, UNREACHABLE
9. `test_scheduler_integration` — API key, appctl --wait, permissions

## What NOT to Test (avoid wasting coverage on)
- Generated code (serde derives, sqlx macros)
- Simple getters/setters
- Third-party library behavior
- CSS/styling (frontend)
- Exact pixel positions in React Flow

## Coverage Reporting
- HTML report generated in `coverage/` directory
- LCOV uploaded to Codecov on every PR
- PR comment shows coverage diff
- CI fails if coverage drops below thresholds
