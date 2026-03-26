//! Transaction Submission Service
//!
//! Main service implementation for submitting transactions to the Stellar network.

use super::error::{SubmissionError, SubmissionResult};
use super::logging::{SubmissionLog, SubmissionLogger, SubmissionTracker};
use super::types::{
    SubmissionConfig, SubmissionRequest, SubmissionResponse, SubmissionStatus, TransactionResult,
};
use crate::horizon_error::HorizonError;
use crate::horizon_retry::{calculate_backoff, RetryConfig, RetryPolicy};
use log::{debug, error, info, warn};
use reqwest::{Client, StatusCode};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Service for submitting transactions to the Stellar network
pub struct TransactionSubmissionService {
    /// Configuration
    config: SubmissionConfig,
    /// HTTP client
    http_client: Client,
    /// Logger for submission attempts
    logger: SubmissionLogger,
    /// Tracker for duplicate detection
    tracker: SubmissionTracker,
    /// Retry configuration
    retry_config: RetryConfig,
    /// Retry policy
    retry_policy: RetryPolicy,
}

impl TransactionSubmissionService {
    /// Create a new submission service with default configuration
    pub fn new() -> SubmissionResult<Self> {
        Self::with_config(SubmissionConfig::default())
    }

    /// Create a new submission service with custom configuration
    pub fn with_config(config: SubmissionConfig) -> SubmissionResult<Self> {
        let http_client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| SubmissionError::InternalError {
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        let logger = if let Some(log_path) = &config.log_path {
            SubmissionLogger::new(log_path.clone())
        } else {
            SubmissionLogger::memory_only()
        };

        let retry_config = RetryConfig {
            max_attempts: config.max_retries + 1, // +1 for initial attempt
            initial_backoff: config.retry_backoff,
            max_backoff: config.max_retry_backoff,
            backoff_multiplier: 2.0,
            use_jitter: true,
        };

        Ok(Self {
            config,
            http_client,
            logger,
            tracker: SubmissionTracker::new(),
            retry_config,
            retry_policy: RetryPolicy::TransientAndServerErrors,
        })
    }

    /// Create a service for testnet
    pub fn testnet() -> SubmissionResult<Self> {
        Self::with_config(SubmissionConfig::testnet())
    }

    /// Create a service for mainnet
    pub fn mainnet() -> SubmissionResult<Self> {
        Self::with_config(SubmissionConfig::mainnet())
    }

    /// Get the service configuration
    pub fn config(&self) -> &SubmissionConfig {
        &self.config
    }

    /// Submit a transaction to the network
    pub async fn submit(&self, request: SubmissionRequest) -> SubmissionResponse {
        let start_time = Instant::now();
        let request_id = request.request_id.clone();

        info!(
            "[{}] Starting transaction submission",
            request_id
        );

        // Create initial log entry
        let mut log = SubmissionLog::from_request(&request);
        log.mark_started();
        if let Err(e) = self.logger.log_attempt(&log) {
            warn!("[{}] Failed to log submission attempt: {}", request_id, e);
        }

        // Check for timeout before starting
        if request.is_timed_out() {
            let response = SubmissionResponse::timeout(&request_id, 0);
            self.finalize_log(&mut log, &response, start_time.elapsed());
            return response;
        }

        // Attempt submission with retries
        let result = self.submit_with_retries(request, &mut log).await;

        // Finalize logging
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                let error_code = error.error_code().to_string();
                let error_message = error.to_string();
                SubmissionResponse::failed(&request_id, error_message, Some(error_code))
            }
        };

        self.finalize_log(&mut log, &response, start_time.elapsed());
        response
    }

    /// Submit with retry logic
    async fn submit_with_retries(
        &self,
        request: SubmissionRequest,
        log: &mut SubmissionLog,
    ) -> SubmissionResult<SubmissionResponse> {
        let mut last_error = None;

        for attempt in 1..=self.retry_config.max_attempts {
            debug!(
                "[{}] Submission attempt {}/{}",
                request.request_id, attempt, self.retry_config.max_attempts
            );

            log.mark_retrying(attempt);
            if let Err(e) = self.logger.log_attempt(log) {
                warn!("Failed to log retry attempt: {}", e);
            }

            match self.submit_single(&request, attempt).await {
                Ok(response) => {
                    info!(
                        "[{}] Transaction submitted successfully on attempt {}",
                        request.request_id, attempt
                    );
                    return Ok(response);
                }
                Err(error) => {
                    warn!(
                        "[{}] Submission attempt {} failed: {}",
                        request.request_id, attempt, error
                    );

                    // Check if we should retry
                    if !self.should_retry(&error, attempt) {
                        return Err(error);
                    }

                    last_error = Some(error);

                    // Calculate backoff and wait before retry
                    if attempt < self.retry_config.max_attempts {
                        let backoff = calculate_backoff(attempt, &self.retry_config);
                        info!(
                            "[{}] Retrying after {:?}...",
                            request.request_id, backoff
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        // All retries exhausted
        Err(SubmissionError::MaxRetriesExceeded {
            attempts: self.retry_config.max_attempts,
            last_error: last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string()),
        })
    }

    /// Submit a single transaction attempt
    async fn submit_single(
        &self,
        request: &SubmissionRequest,
        attempt: u32,
    ) -> SubmissionResult<SubmissionResponse> {
        // Check for timeout
        if request.is_timed_out() {
            return Err(SubmissionError::Timeout {
                duration: request.elapsed(),
                attempts: attempt,
            });
        }

        // Build submission URL
        let url = format!("{}/transactions", self.config.horizon_url);

        // Prepare request body
        let body = json!({
            "tx": request.signed_xdr
        });

        debug!(
            "[{}] POST {} (attempt {})",
            request.request_id, url, attempt
        );

        // Make the submission request with timeout
        let response = match timeout(
            self.config.timeout,
            self.http_client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => {
                return Err(self.handle_request_error(e));
            }
            Err(_) => {
                return Err(SubmissionError::Timeout {
                    duration: self.config.timeout,
                    attempts: attempt,
                });
            }
        };

        let status = response.status();
        let response_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read response body".to_string());

        debug!(
            "[{}] Response status: {}, body: {}",
            request.request_id, status, response_body
        );

        // Handle response based on status code
        match status {
            StatusCode::OK => self.handle_success_response(&request.request_id, &response_body),
            StatusCode::BAD_REQUEST => {
                self.handle_error_response(StatusCode::BAD_REQUEST.as_u16(), &response_body)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                // Rate limited
                let retry_after = Duration::from_secs(60);
                Err(SubmissionError::RateLimited { retry_after })
            }
            StatusCode::NOT_FOUND => Err(SubmissionError::ServerError {
                status: 404,
                message: "Horizon endpoint not found".to_string(),
            }),
            s if s.is_server_error() => Err(SubmissionError::ServerError {
                status: s.as_u16(),
                message: response_body,
            }),
            s => Err(SubmissionError::ServerError {
                status: s.as_u16(),
                message: format!("Unexpected status: {}", response_body),
            }),
        }
    }

    /// Handle successful response from Horizon
    fn handle_success_response(
        &self,
        request_id: &str,
        response_body: &str,
    ) -> SubmissionResult<SubmissionResponse> {
        let json: serde_json::Value = serde_json::from_str(response_body).map_err(|e| {
            SubmissionError::InvalidResponse {
                message: format!("Failed to parse success response: {}", e),
            }
        })?;

        // Extract transaction hash
        let transaction_hash = json
            .get("hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SubmissionError::InvalidResponse {
                message: "Missing transaction hash in response".to_string(),
            })?;

        // Extract ledger sequence
        let ledger_sequence = json.get("ledger").and_then(|v| v.as_u64()).map(|v| v as u32);

        // Check if successful
        let successful = json
            .get("successful")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !successful {
            return Err(SubmissionError::TransactionFailed {
                result_code: "tx_failed".to_string(),
                message: "Transaction was included in ledger but marked as failed".to_string(),
                operation_results: vec![],
            });
        }

        // Track the successful submission
        self.tracker
            .track(transaction_hash, request_id);
        self.tracker
            .update_status(transaction_hash, SubmissionStatus::Success);

        info!(
            "[{}] Transaction {} confirmed in ledger {:?}",
            request_id, transaction_hash, ledger_sequence
        );

        Ok(SubmissionResponse::success(
            request_id,
            transaction_hash,
            ledger_sequence.unwrap_or(0),
        ))
    }

    /// Handle error response from Horizon
    fn handle_error_response(
        &self,
        status: u16,
        response_body: &str,
    ) -> SubmissionResult<SubmissionResponse> {
        // Parse the error using the error module
        let error = SubmissionError::from_horizon_response(status, response_body);

        // Check for specific error types
        match &error {
            SubmissionError::DuplicateTransaction { transaction_hash, .. } => {
                // Transaction was already submitted
                warn!(
                    "Transaction {} already submitted",
                    transaction_hash
                );
            }
            SubmissionError::InvalidSequence { message, .. } => {
                warn!("Sequence number error: {}", message);
            }
            SubmissionError::InsufficientBalance { message, .. } => {
                warn!("Insufficient balance: {}", message);
            }
            _ => {}
        }

        Err(error)
    }

    /// Handle request errors
    fn handle_request_error(&self, error: reqwest::Error) -> SubmissionError {
        if error.is_timeout() {
            SubmissionError::Timeout {
                duration: self.config.timeout,
                attempts: 1,
            }
        } else if error.is_connect() {
            SubmissionError::NetworkError {
                message: format!("Connection failed: {}", error),
                is_retryable: true,
            }
        } else if error.is_request() {
            SubmissionError::NetworkError {
                message: format!("Request error: {}", error),
                is_retryable: true,
            }
        } else {
            SubmissionError::NetworkError {
                message: error.to_string(),
                is_retryable: true,
            }
        }
    }

    /// Determine if we should retry based on the error
    fn should_retry(&self, error: &SubmissionError, attempt: u32) -> bool {
        // Don't retry if we've reached max attempts
        if attempt >= self.retry_config.max_attempts {
            return false;
        }

        // Don't retry balance errors - they won't succeed
        if error.is_balance_error() {
            return false;
        }

        // Retry sequence errors (might succeed with new sequence)
        if error.is_sequence_error() {
            return true;
        }

        // Check general retryability
        error.is_retryable()
    }

    /// Finalize logging for a submission
    fn finalize_log(&self, log: &mut SubmissionLog, response: &SubmissionResponse, duration: Duration) {
        log.update_from_response(response, duration.as_millis() as u64);

        if let Err(e) = self.logger.log_attempt(log) {
            warn!("Failed to finalize submission log: {}", e);
        }

        // Log summary
        match response.status {
            SubmissionStatus::Success => {
                info!(
                    "[{}] Submission completed successfully in {}ms (hash: {:?})",
                    response.request_id,
                    duration.as_millis(),
                    response.transaction_hash
                );
            }
            SubmissionStatus::Duplicate => {
                info!(
                    "[{}] Duplicate transaction detected (hash: {:?})",
                    response.request_id,
                    response.transaction_hash
                );
            }
            _ => {
                error!(
                    "[{}] Submission failed after {}ms: {:?}",
                    response.request_id,
                    duration.as_millis(),
                    response.error_message
                );
            }
        }
    }

    /// Check if a transaction has been submitted before
    pub fn is_duplicate(&self, transaction_hash: &str) -> bool {
        self.tracker.is_tracked(transaction_hash)
            || self.logger.is_duplicate(transaction_hash)
    }

    /// Get submission statistics
    pub fn get_stats(&self) -> super::logging::LogStats {
        self.logger.get_stats()
    }

    /// Get recent submissions
    pub fn get_recent_submissions(&self) -> Vec<SubmissionLog> {
        self.logger.get_recent_logs()
    }

    /// Clear all logs and tracking
    pub fn clear(&self) -> anyhow::Result<()> {
        self.logger.clear()?;
        self.tracker.clear();
        Ok(())
    }
}

impl Default for TransactionSubmissionService {
    fn default() -> Self {
        Self::new().expect("Failed to create default TransactionSubmissionService")
    }
}

/// Builder for creating submission services
pub struct SubmissionServiceBuilder {
    config: SubmissionConfig,
}

impl SubmissionServiceBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: SubmissionConfig::default(),
        }
    }

    /// Set Horizon URL
    pub fn horizon_url(mut self, url: impl Into<String>) -> Self {
        self.config.horizon_url = url.into();
        self
    }

    /// Set timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Set max retries
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    /// Set log path
    pub fn log_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.config.log_path = Some(path.into());
        self
    }

    /// Build the service
    pub fn build(self) -> SubmissionResult<TransactionSubmissionService> {
        TransactionSubmissionService::with_config(self.config)
    }
}

impl Default for SubmissionServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = TransactionSubmissionService::new();
        assert!(service.is_ok());
    }

    #[test]
    fn test_service_config() {
        let service = TransactionSubmissionService::testnet().unwrap();
        assert!(service.config().horizon_url.contains("testnet"));
    }

    #[test]
    fn test_submission_request_builder() {
        let request = SubmissionRequest::new("test_xdr")
            .with_timeout(Duration::from_secs(30))
            .with_retries(5)
            .with_memo("test donation");

        assert_eq!(request.timeout, Duration::from_secs(30));
        assert_eq!(request.max_retries, 5);
        assert_eq!(request.memo, Some("test donation".to_string()));
    }

    #[test]
    fn test_should_retry_logic() {
        let service = TransactionSubmissionService::testnet().unwrap();

        // Network errors should be retried
        let network_error = SubmissionError::NetworkError {
            message: "connection failed".to_string(),
            is_retryable: true,
        };
        assert!(service.should_retry(&network_error, 1));

        // Balance errors should not be retried
        let balance_error = SubmissionError::InsufficientBalance {
            message: "no funds".to_string(),
            account: "G...".to_string(),
            required: None,
            available: None,
        };
        assert!(!service.should_retry(&balance_error, 1));

        // Sequence errors should be retried
        let seq_error = SubmissionError::InvalidSequence {
            message: "bad seq".to_string(),
            current_sequence: None,
            expected_sequence: None,
        };
        assert!(service.should_retry(&seq_error, 1));
    }

    #[test]
    fn test_service_builder() {
        let service = SubmissionServiceBuilder::new()
            .horizon_url("https://custom.horizon.org")
            .timeout(Duration::from_secs(45))
            .max_retries(5)
            .build();

        assert!(service.is_ok());
        let service = service.unwrap();
        assert_eq!(service.config().horizon_url, "https://custom.horizon.org");
        assert_eq!(service.config().timeout, Duration::from_secs(45));
        assert_eq!(service.config().max_retries, 5);
    }
}
