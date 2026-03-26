//! Transaction Submission Service
//!
//! Provides a robust service for submitting signed transactions to the Stellar network
//! via Horizon API. Features include:
//! - Transaction submission with retry logic
//! - Response handling and transaction hash extraction
//! - Error categorization (insufficient funds, bad sequence, etc.)
//! - Duplicate transaction detection
//! - Submission logging and timeout handling

pub mod error;
pub mod logging;
pub mod service;
pub mod types;

pub use error::{SubmissionError, SubmissionResult};
pub use logging::{SubmissionLog, SubmissionLogger};
pub use service::{TransactionSubmissionService, SubmissionConfig};
pub use types::{SubmissionRequest, SubmissionResponse, SubmissionStatus, TransactionResult};
