# CLAUDE.md - tests/

## Purpose
End-to-end integration tests that validate complete scenarios across all components. Tests run against a real PostgreSQL 16 instance (Docker in CI).

## Tech
- Rust integration tests using `tokio::test`
- Test database: PostgreSQL 16 (docker-compose or CI service)
- Each test gets a fresh database (run migrations, insert fixtures, test, drop)
- HTTP client: `reqwest` for API calls
- WebSocket client: `tokio-tungstenite` for realtime assertions

## Test Fixtures
Create a reusable `TestContext` struct:
```rust
struct TestContext {
    db_pool: PgPool,
    api_url: String,
    ws_url: String,
    admin_token: String,
    operator_token: String,
    viewer_token: String,
    org_id: Uuid,
}

impl TestContext {
    async fn new() -> Self { /* create temp DB, run migrations, seed users */ }
    async fn create_test_app(&self, name: &str, components: Vec<TestComponent>) -> Uuid { /* insert app + components + deps */ }
    async fn cleanup(&self) { /* drop temp DB */ }
}
```

## Test Files
See `tests/e2e/` directory for all test implementations.
