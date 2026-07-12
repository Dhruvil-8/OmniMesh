//! Retry policies with exponential backoff.
//!
//! Provides a simple, configurable retry mechanism for transient failures.
//! Uses the `backoff` crate under the hood.

use std::future::Future;
use std::time::Duration;

use backoff::ExponentialBackoff;
use tracing::{debug, warn};

use crate::error::Result;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Initial backoff interval.
    pub initial_interval: Duration,
    /// Maximum backoff interval.
    pub max_interval: Duration,
    /// Multiplier applied to the interval after each retry.
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_interval: Duration::from_millis(100),
            max_interval: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Create a policy for aggressive retries (short intervals, many attempts).
    pub fn aggressive() -> Self {
        Self {
            max_retries: 10,
            initial_interval: Duration::from_millis(50),
            max_interval: Duration::from_secs(5),
            multiplier: 1.5,
        }
    }

    /// Create a policy for conservative retries (long intervals, few attempts).
    pub fn conservative() -> Self {
        Self {
            max_retries: 3,
            initial_interval: Duration::from_secs(1),
            max_interval: Duration::from_secs(60),
            multiplier: 3.0,
        }
    }

    /// No retries — fail immediately.
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Convert to a `backoff::ExponentialBackoff`.
    fn to_backoff(&self) -> ExponentialBackoff {
        ExponentialBackoff {
            initial_interval: self.initial_interval,
            max_interval: self.max_interval,
            multiplier: self.multiplier,
            max_elapsed_time: None,
            ..Default::default()
        }
    }
}

/// Retry an async operation according to the given policy.
///
/// Only retries if the error is classified as retryable
/// (see [`OmniMeshError::is_retryable`]).
///
/// # Example
/// ```ignore
/// use omnimesh_core::retry::{retry_async, RetryPolicy};
///
/// let result = retry_async(&RetryPolicy::default(), || async {
///     some_fallible_operation().await
/// }).await;
/// ```
pub async fn retry_async<F, Fut, T>(policy: &RetryPolicy, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if policy.max_retries == 0 {
        return operation().await;
    }

    let mut attempts = 0u32;
    let backoff = policy.to_backoff();
    let mut current_interval = backoff.initial_interval;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) if err.is_retryable() && attempts < policy.max_retries => {
                attempts += 1;
                warn!(
                    attempt = attempts,
                    max_retries = policy.max_retries,
                    error = %err,
                    backoff_ms = current_interval.as_millis() as u64,
                    "retrying after transient error"
                );
                tokio::time::sleep(current_interval).await;
                current_interval =
                    Duration::from_secs_f64(current_interval.as_secs_f64() * backoff.multiplier)
                        .min(backoff.max_interval);
            }
            Err(err) => {
                if attempts > 0 {
                    debug!(
                        total_attempts = attempts + 1,
                        "operation failed after all retries"
                    );
                }
                return Err(err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::OmniMeshError;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_immediately() {
        let result = retry_async(&RetryPolicy::default(), || async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let policy = RetryPolicy {
            max_retries: 3,
            initial_interval: Duration::from_millis(1),
            max_interval: Duration::from_millis(10),
            multiplier: 2.0,
        };

        let result: Result<&str> = retry_async(&policy, || {
            let c = counter_clone.clone();
            async move {
                let attempt = c.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    Err(OmniMeshError::Connection("refused".into()))
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_interval: Duration::from_millis(1),
            max_interval: Duration::from_millis(10),
            multiplier: 2.0,
        };

        let result: Result<()> = retry_async(&policy, || async {
            Err(OmniMeshError::Connection("refused".into()))
        })
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_non_retryable_error_fails_immediately() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<()> = retry_async(&RetryPolicy::default(), || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(OmniMeshError::Crypto("bad key".into()))
            }
        })
        .await;

        assert!(result.is_err());
        // Non-retryable errors should not be retried
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
