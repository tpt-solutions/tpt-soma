//! Per-subject rate limit for multipart uploads.
//!
//! A token bucket per authenticated subject. Each subject may burst up to
//! `capacity` uploads, then gets one more every `window / capacity`. The limit
//! is per user, so one researcher's uploads cannot use up another's.
//!
//! State lives in this process only. Running several API replicas gives each
//! its own buckets, so the effective limit grows with the replica count.

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tpt_soma_capability::token::CapabilityToken;

/// Uploads a subject may send in a burst, and the most it may send per window.
pub const UPLOAD_BURST: u32 = 10;
pub const UPLOAD_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct UploadRateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl UploadRateLimiter {
    pub fn new(capacity: u32, window: Duration) -> Self {
        let capacity = f64::from(capacity);
        Self {
            capacity,
            refill_per_sec: capacity / window.as_secs_f64(),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one upload for `subject`. When refused, returns how long until the
    /// next one is allowed.
    pub fn try_acquire(&self, subject: &str, now: Instant) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let bucket = buckets.entry(subject.to_string()).or_insert(Bucket {
            tokens: self.capacity,
            last: now,
        });

        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.last = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(Duration::from_secs_f64(
                (1.0 - bucket.tokens) / self.refill_per_sec,
            ))
        }
    }
}

/// Applies the limiter to the authenticated subject. It must sit inside
/// `capability_middleware`, which puts the token in the request extensions.
pub async fn upload_rate_limit(
    State(limiter): State<Arc<UploadRateLimiter>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(subject) = req
        .extensions()
        .get::<CapabilityToken>()
        .map(|token| token.subject.clone())
    else {
        return (StatusCode::UNAUTHORIZED, "missing authenticated subject").into_response();
    };

    match limiter.try_acquire(&subject, Instant::now()) {
        Ok(()) => next.run(req).await,
        Err(wait) => {
            let retry_after = wait.as_secs_f64().ceil().max(1.0) as u64;
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after.to_string())],
                "upload rate limit exceeded",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, middleware, routing::post};
    use tower::ServiceExt;

    #[test]
    fn burst_is_allowed_then_refused() {
        let limiter = UploadRateLimiter::new(3, Duration::from_secs(60));
        let now = Instant::now();
        assert!(limiter.try_acquire("alice", now).is_ok());
        assert!(limiter.try_acquire("alice", now).is_ok());
        assert!(limiter.try_acquire("alice", now).is_ok());
        assert!(limiter.try_acquire("alice", now).is_err());
    }

    #[test]
    fn refused_call_reports_wait_until_next_token() {
        // 1 upload per 60 s: after the burst, the next one is 60 s away.
        let limiter = UploadRateLimiter::new(1, Duration::from_secs(60));
        let now = Instant::now();
        limiter.try_acquire("alice", now).unwrap();
        let wait = limiter.try_acquire("alice", now).unwrap_err();
        assert!(
            (wait.as_secs_f64() - 60.0).abs() < 1e-6,
            "wait was {wait:?}"
        );
    }

    #[test]
    fn tokens_refill_over_time() {
        let limiter = UploadRateLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();
        limiter.try_acquire("alice", start).unwrap();
        limiter.try_acquire("alice", start).unwrap();
        assert!(limiter.try_acquire("alice", start).is_err());
        // Half the window refills one of the two tokens.
        let later = start + Duration::from_secs(30);
        assert!(limiter.try_acquire("alice", later).is_ok());
        assert!(limiter.try_acquire("alice", later).is_err());
    }

    #[test]
    fn subjects_have_independent_buckets() {
        let limiter = UploadRateLimiter::new(1, Duration::from_secs(60));
        let now = Instant::now();
        limiter.try_acquire("alice", now).unwrap();
        assert!(limiter.try_acquire("alice", now).is_err());
        assert!(limiter.try_acquire("bob", now).is_ok());
    }

    fn token_for(subject: &str) -> CapabilityToken {
        CapabilityToken {
            subject: subject.to_string(),
            resource_class: "genomic_raw".to_string(),
            cohort_scope: vec!["*".to_string()],
            action: "write".to_string(),
            expiry: u64::MAX,
            nonce: vec![1u8; 32],
            signature: Vec::new(),
            graph_scope: None,
        }
    }

    fn upload_app(limiter: Arc<UploadRateLimiter>, subject: Option<&'static str>) -> Router {
        Router::new()
            .route("/up", post(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(limiter, upload_rate_limit))
            // Stands in for capability_middleware: puts the token in the extensions.
            .layer(middleware::from_fn(
                move |mut req: Request, next: Next| async move {
                    if let Some(subject) = subject {
                        req.extensions_mut().insert(token_for(subject));
                    }
                    next.run(req).await
                },
            ))
    }

    fn post_up() -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/up")
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn refuses_with_429_and_retry_after_once_burst_is_spent() {
        let app = upload_app(
            Arc::new(UploadRateLimiter::new(1, Duration::from_secs(60))),
            Some("alice"),
        );
        let first = app.clone().oneshot(post_up()).await.unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app.oneshot(post_up()).await.unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()[header::RETRY_AFTER], "60");
    }

    #[tokio::test]
    async fn refuses_requests_without_an_authenticated_subject() {
        let app = upload_app(
            Arc::new(UploadRateLimiter::new(10, Duration::from_secs(60))),
            None,
        );
        let response = app.oneshot(post_up()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
