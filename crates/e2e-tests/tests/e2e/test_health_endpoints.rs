/// E2E Test: Health and Readiness Endpoints
///
/// Validates:
/// - GET /health returns 200 (no auth required)
/// - GET /ready returns 200 when DB is connected
/// - Endpoints are accessible without JWT
use super::*;

#[cfg(test)]
mod test_health_endpoints {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let ctx = TestContext::new().await;

        let resp = ctx.get_anonymous("/health").await;
        assert_eq!(resp.status(), 200, "Health endpoint should return 200");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_ready_endpoint() {
        let ctx = TestContext::new().await;

        let resp = ctx.get_anonymous("/ready").await;
        assert_eq!(
            resp.status(),
            200,
            "Ready endpoint should return 200 when DB is healthy"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_health_no_auth_required() {
        let ctx = TestContext::new().await;

        // No Authorization header at all
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/health", ctx.api_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "Health should not require auth");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_api_endpoints_require_auth() {
        let ctx = TestContext::new().await;

        // API endpoints without auth → 401
        let resp = ctx.get_anonymous("/api/v1/apps").await;
        assert_eq!(resp.status(), 401, "API endpoints should require auth");

        ctx.cleanup().await;
    }
}
