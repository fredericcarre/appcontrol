/// SQLite E2E: Health endpoints (mirrors test_health_endpoints.rs)
use super::common::TestContext;

#[tokio::test]
async fn test_health_returns_200() {
    let ctx = TestContext::new().await;
    let resp = ctx.get_anonymous("/health").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_ready_returns_200() {
    let ctx = TestContext::new().await;
    let resp = ctx.get_anonymous("/ready").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_api_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get_anonymous("/api/v1/apps").await;
    assert_eq!(resp.status(), 401);
}
