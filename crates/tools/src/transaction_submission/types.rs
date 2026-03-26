//! Transaction Submission Types
//!
//! Core types for transaction submission requests and responses.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// Status of a transaction submission
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionStatus {
    /// Submission is pending
    Pending,
    /// Transaction submitted successfully
    Success,
    /// Submission failed with an error
    Failed,
    /// Submission timed out
    Timeout,
    /// Transaction was a duplicate
    Duplicate,
    /// Submission is being retried
    Retrying,
}

impl std::fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmissionStatus::Pending => write!(f, "pending"),
            SubmissionStatus::Success => write!(f, "success"),
            SubmissionStatus::Failed => write!(f, "failed"),
            SubmissionStatus::Timeout => write!(f, "timeout"),
            SubmissionStatus::Duplicate => write!(f, "duplicate"),
            SubmissionStatus::Retrying => write!(f, "retrying"),
        }
    }
}

/// Request to submit a transaction
#[derive(Debug, Clone)]
pub struct SubmissionRequest {
    /// The signed transaction envelope XDR (base64 encoded)
    pub signed_xdr: String,
    /// Unique identifier for this submission request
    pub request_id: String,
    /// Maximum time to wait for submission
    pub timeout: Duration,
    /// Whether to retry on transient failures
    pub enable_retries: bool,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Optional memo for tracking
    pub memo: Option<String>,
    /// Timestamp when request was created
    pub created_at: SystemTime,
}

impl SubmissionRequest {
    /// Create a new submission request
    pub fn new(signed_xdr: impl Into<String>) -> Self {
        Self {
            signed_xdr: signed_xdr.into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            timeout: Duration::from_secs(60),
            enable_retries: true,
            max_retries: 3,
            memo: None,
            created_at: SystemTime::now(),
        }
    }

    /// Set a custom timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set retry configuration
    pub fn with_retries(mut self, max_retries: u32) -> Self {
        self.enable_retries = max_retries > 0;
        self.max_retries = max_retries;
        self
    }

    /// Disable retries
    pub fn without_retries(mut self) -> Self {
        self.enable_retries = false;
        self.max_retries = 0;
        self
    }

    /// Set a memo for tracking
    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }

    /// Set a custom request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    /// Check if the request has timed out
    pub fn is_timed_out(&self) -> bool {
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or(Duration::MAX)
            > self.timeout
    }

    /// Get elapsed time since request creation
    pub fn elapsed(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or(Duration::ZERO)
    }
}

impl Default for SubmissionRequest {
    fn default() -> Self {
        Self::new("")
    }
}

/// Result of a transaction submission
#[derive(Debug, Clone)]
pub struct SubmissionResponse {
    /// The request ID
    pub request_id: String,
    /// Submission status
    pub status: SubmissionStatus,
    /// Transaction hash (if successful)
    pub transaction_hash: Option<String>,
    /// Ledger sequence the transaction was included in (if successful)
    pub ledger_sequence: Option<u32>,
    /// Timestamp when submission completed
    pub completed_at: Option<SystemTime>,
    /// Number of submission attempts made
    pub attempts: u32,
    /// Error message (if failed)
    pub error_message: Option<String>,
    /// Error code (if failed)
    pub error_code: Option<String>,
    /// Additional result details
    pub result: Option<TransactionResult>,
}

impl SubmissionResponse {
    /// Create a successful response
    pub fn success(
        request_id: impl Into<String>,
        transaction_hash: impl Into<String>,
        ledger_sequence: u32,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            status: SubmissionStatus::Success,
            transaction_hash: Some(transaction_hash.into()),
            ledger_sequence: Some(ledger_sequence),
            completed_at: Some(SystemTime::now()),
            attempts: 1,
            error_message: None,
            error_code: None,
            result: None,
        }
    }

    /// Create a failed response
    pub fn failed(
        request_id: impl Into<String>,
        error_message: impl Into<String>,
        error_code: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            status: SubmissionStatus::Failed,
            transaction_hash: None,
            ledger_sequence: None,
            completed_at: Some(SystemTime::now()),
            attempts: 1,
            error_message: Some(error_message.into()),
            error_code,
            result: None,
        }
    }

    /// Create a timeout response
    pub fn timeout(request_id: impl Into<String>, attempts: u32) -> Self {
        Self {
            request_id: request_id.into(),
            status: SubmissionStatus::Timeout,
            transaction_hash: None,
            ledger_sequence: None,
            completed_at: Some(SystemTime::now()),
            attempts,
            error_message: Some("Submission timed out".to_string()),
            error_code: Some("tx_timeout".to_string()),
            result: None,
        }
    }

    /// Create a duplicate response
    pub fn duplicate(
        request_id: impl Into<String>,
        transaction_hash: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            status: SubmissionStatus::Duplicate,
            transaction_hash: Some(transaction_hash.into()),
            ledger_sequence: None,
            completed_at: Some(SystemTime::now()),
            attempts: 1,
            error_message: Some("Transaction already submitted".to_string()),
            error_code: Some("tx_duplicate".to_string()),
            result: None,
        }
    }

    /// Check if submission was successful
    pub fn is_success(&self) -> bool {
        self.status == SubmissionStatus::Success
    }

    /// Check if submission failed
    pub fn is_failed(&self) -> bool {
        matches!(
            self.status,
            SubmissionStatus::Failed | SubmissionStatus::Timeout
        )
    }

    /// Get the transaction hash if available
    pub fn get_transaction_hash(&self) -> Option<&str> {
        self.transaction_hash.as_deref()
    }
}

/// Detailed transaction result from Horizon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    /// Transaction hash
    pub hash: String,
    /// Ledger sequence
    pub ledger: u32,
    /// Whether the transaction was successful
    pub successful: bool,
    /// Result code
    pub result_code: String,
    /// Result code description
    pub result_code_description: Option<String>,
    /// Operation results (if any)
    pub operation_results: Vec<OperationResult>,
    /// Transaction envelope XDR
    pub envelope_xdr: Option<String>,
    /// Result XDR
    pub result_xdr: Option<String>,
    /// Meta XDR
    pub meta_xdr: Option<String>,
}

/// Operation result within a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    /// Operation index
    pub index: u32,
    /// Whether the operation was successful
    pub successful: bool,
    /// Result code
    pub result_code: String,
    /// Result description
    pub result_description: Option<String>,
}

/// Configuration for transaction submission
#[derive(Debug, Clone)]
pub struct SubmissionConfig {
    /// Base URL for Horizon API
    pub horizon_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Maximum retry attempts for transient failures
    pub max_retries: u32,
    /// Initial retry backoff
    pub retry_backoff: Duration,
    /// Maximum retry backoff
    pub max_retry_backoff: Duration,
    /// Whether to enable duplicate detection
    pub enable_duplicate_detection: bool,
    /// Path for submission logs
    pub log_path: Option<std::path::PathBuf>,
}

impl Default for SubmissionConfig {
    fn default() -> Self {
        Self {
            horizon_url: "https://horizon.stellar.org".to_string(),
            timeout: Duration::from_secs(60),
            max_retries: 3,
            retry_backoff: Duration::from_millis(500),
            max_retry_backoff: Duration::from_secs(10),
            enable_duplicate_detection: true,
            log_path: None,
        }
    }
}

impl SubmissionConfig {
    /// Create configuration for testnet
    pub fn testnet() -> Self {
        Self {
            horizon_url: "https://horizon-testnet.stellar.org".to_string(),
            ..Default::default()
        }
    }

    /// Create configuration for mainnet
    pub fn mainnet() -> Self {
        Self {
            horizon_url: "https://horizon.stellar.org".to_string(),
            ..Default::default()
        }
    }

    /// Create configuration for local testing
    pub fn local() -> Self {
        Self {
            horizon_url: "http://localhost:8000".to_string(),
            timeout: Duration::from_secs(10),
            max_retries: 1,
            ..Default::default()
        }
    }

    /// Set custom Horizon URL
    pub fn with_horizon_url(mut self, url: impl Into<String>) -> Self {
        self.horizon_url = url.into();
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set retry configuration
    pub fn with_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set log path
    pub fn with_log_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.log_path = Some(path.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submission_request_creation() {
        let request = SubmissionRequest::new("test_xdr");
        assert!(!request.request_id.is_empty());
        assert_eq!(request.signed_xdr, "test_xdr");
        assert!(request.enable_retries);
    }

    #[test]
    fn test_submission_request_timeout() {
        let request = SubmissionRequest::new("test").with_timeout(Duration::from_secs(30));
        assert_eq!(request.timeout, Duration::from_secs(30));
        assert!(!request.is_timed_out());
    }

    #[test]
    fn test_submission_response_success() {
        let response = SubmissionResponse::success("req-123", "tx-hash", 12345);
        assert!(response.is_success());
        assert_eq!(response.get_transaction_hash(), Some("tx-hash"));
        assert_eq!(response.ledger_sequence, Some(12345));
    }

    #[test]
    fn test_submission_response_failed() {
        let response =
            SubmissionResponse::failed("req-123", "insufficient funds", Some("tx_insufficient_balance".to_string()));
        assert!(!response.is_success());
        assert!(response.is_failed());
        assert_eq!(response.error_code, Some("tx_insufficient_balance".to_string()));
    }

    #[test]
    fn test_submission_status_display() {
        assert_eq!(SubmissionStatus::Success.to_string(), "success");
        assert_eq!(SubmissionStatus::Failed.to_string(), "failed");
        assert_eq!(SubmissionStatus::Timeout.to_string(), "timeout");
    }

    #[test]
    fn test_submission_config() {
        let config = SubmissionConfig::testnet();
        assert!(config.horizon_url.contains("testnet"));

        let config = SubmissionConfig::mainnet();
        assert!(config.horizon_url.contains("stellar.org"));
        assert!(!config.horizon_url.contains("testnet"));
    }
}
