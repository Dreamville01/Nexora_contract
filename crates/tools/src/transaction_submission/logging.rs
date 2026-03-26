//! Transaction Submission Logging
//!
//! Provides logging capabilities for transaction submission attempts and results.

use super::types::{SubmissionRequest, SubmissionResponse, SubmissionStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Log entry for a submission attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionLog {
    /// Unique log entry ID
    pub log_id: String,
    /// Request ID associated with this log
    pub request_id: String,
    /// Transaction hash (if known)
    pub transaction_hash: Option<String>,
    /// Status of the submission
    pub status: String,
    /// Timestamp when this log entry was created
    pub timestamp: DateTime<Utc>,
    /// Number of attempts made
    pub attempts: u32,
    /// Error code (if any)
    pub error_code: Option<String>,
    /// Error message (if any)
    pub error_message: Option<String>,
    /// Ledger sequence (if successful)
    pub ledger_sequence: Option<u32>,
    /// Duration of the submission process
    pub duration_ms: u64,
    /// Additional metadata
    #[serde(flatten)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SubmissionLog {
    /// Create a new log entry from a request
    pub fn from_request(request: &SubmissionRequest) -> Self {
        Self {
            log_id: uuid::Uuid::new_v4().to_string(),
            request_id: request.request_id.clone(),
            transaction_hash: None,
            status: "pending".to_string(),
            timestamp: Utc::now(),
            attempts: 0,
            error_code: None,
            error_message: None,
            ledger_sequence: None,
            duration_ms: 0,
            metadata: HashMap::new(),
        }
    }

    /// Update log from a response
    pub fn update_from_response(&mut self, response: &SubmissionResponse, duration_ms: u64) {
        self.status = response.status.to_string();
        self.transaction_hash = response.transaction_hash.clone();
        self.ledger_sequence = response.ledger_sequence;
        self.error_code = response.error_code.clone();
        self.error_message = response.error_message.clone();
        self.attempts = response.attempts;
        self.duration_ms = duration_ms;
    }

    /// Mark as started
    pub fn mark_started(&mut self) {
        self.status = "in_progress".to_string();
        self.timestamp = Utc::now();
    }

    /// Mark as retrying
    pub fn mark_retrying(&mut self, attempt: u32) {
        self.status = "retrying".to_string();
        self.attempts = attempt;
    }

    /// Add metadata
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Convert to pretty JSON string
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Logger for transaction submissions
pub struct SubmissionLogger {
    /// Path to the log file
    log_path: PathBuf,
    /// In-memory buffer of recent logs
    recent_logs: Arc<Mutex<Vec<SubmissionLog>>>,
    /// Maximum number of recent logs to keep in memory
    max_recent_logs: usize,
}

impl SubmissionLogger {
    /// Create a new logger with the specified log file path
    pub fn new(log_path: impl Into<PathBuf>) -> Self {
        let log_path = log_path.into();
        Self {
            log_path,
            recent_logs: Arc::new(Mutex::new(Vec::new())),
            max_recent_logs: 1000,
        }
    }

    /// Create a logger with default path
    pub fn default_path() -> Self {
        Self::new(".transaction_submissions.jsonl")
    }

    /// Create a logger that only logs to memory (no file)
    pub fn memory_only() -> Self {
        Self {
            log_path: PathBuf::new(),
            recent_logs: Arc::new(Mutex::new(Vec::new())),
            max_recent_logs: 10000,
        }
    }

    /// Log a submission attempt
    pub fn log_attempt(&self, log: &SubmissionLog) -> anyhow::Result<()> {
        // Add to recent logs
        {
            let mut recent = self.recent_logs.lock().unwrap();
            recent.push(log.clone());
            if recent.len() > self.max_recent_logs {
                recent.remove(0);
            }
        }

        // Write to file if path is set
        if !self.log_path.as_os_str().is_empty() {
            self.write_to_file(log)?;
        }

        Ok(())
    }

    /// Write log entry to file
    fn write_to_file(&self, log: &SubmissionLog) -> anyhow::Result<()> {
        let json = log.to_json()?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        writeln!(file, "{}", json)?;
        file.flush()?;

        Ok(())
    }

    /// Get recent logs
    pub fn get_recent_logs(&self) -> Vec<SubmissionLog> {
        self.recent_logs.lock().unwrap().clone()
    }

    /// Get logs for a specific request ID
    pub fn get_logs_for_request(&self, request_id: &str) -> Vec<SubmissionLog> {
        self.recent_logs
            .lock()
            .unwrap()
            .iter()
            .filter(|log| log.request_id == request_id)
            .cloned()
            .collect()
    }

    /// Get logs for a specific transaction hash
    pub fn get_logs_for_transaction(&self, transaction_hash: &str) -> Vec<SubmissionLog> {
        self.recent_logs
            .lock()
            .unwrap()
            .iter()
            .filter(|log| {
                log.transaction_hash
                    .as_ref()
                    .map(|h| h == transaction_hash)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Check if a transaction has been submitted before (duplicate detection)
    pub fn is_duplicate(&self, transaction_hash: &str) -> bool {
        self.recent_logs
            .lock()
            .unwrap()
            .iter()
            .any(|log| {
                log.transaction_hash
                    .as_ref()
                    .map(|h| h == transaction_hash)
                    .unwrap_or(false)
                    && (log.status == "success" || log.status == "pending")
            })
    }

    /// Get the most recent log for a transaction
    pub fn get_latest_for_transaction(&self, transaction_hash: &str) -> Option<SubmissionLog> {
        self.recent_logs
            .lock()
            .unwrap()
            .iter()
            .filter(|log| {
                log.transaction_hash
                    .as_ref()
                    .map(|h| h == transaction_hash)
                    .unwrap_or(false)
            })
            .last()
            .cloned()
    }

    /// Load all logs from file
    pub fn load_from_file(&self) -> anyhow::Result<Vec<SubmissionLog>> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.log_path)?;
        let reader = BufReader::new(file);
        let mut logs = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if let Ok(log) = serde_json::from_str::<SubmissionLog>(&line) {
                logs.push(log);
            }
        }

        Ok(logs)
    }

    /// Clear all logs
    pub fn clear(&self) -> anyhow::Result<()> {
        self.recent_logs.lock().unwrap().clear();

        if self.log_path.exists() {
            std::fs::remove_file(&self.log_path)?;
        }

        Ok(())
    }

    /// Get log statistics
    pub fn get_stats(&self) -> LogStats {
        let logs = self.recent_logs.lock().unwrap();

        let total = logs.len();
        let successful = logs.iter().filter(|l| l.status == "success").count();
        let failed = logs.iter().filter(|l| l.status == "failed").count();
        let pending = logs.iter().filter(|l| l.status == "pending").count();
        let duplicates = logs.iter().filter(|l| l.status == "duplicate").count();

        let avg_duration = if total > 0 {
            logs.iter().map(|l| l.duration_ms).sum::<u64>() / total as u64
        } else {
            0
        };

        LogStats {
            total,
            successful,
            failed,
            pending,
            duplicates,
            avg_duration_ms: avg_duration,
        }
    }

    /// Rotate log file if it exceeds max size
    pub fn rotate_if_needed(&self, max_size_bytes: u64) -> anyhow::Result<()> {
        if !self.log_path.exists() {
            return Ok(());
        }

        let metadata = std::fs::metadata(&self.log_path)?;
        if metadata.len() > max_size_bytes {
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
            let rotated_path = self
                .log_path
                .with_extension(format!("jsonl.{}", timestamp));
            std::fs::rename(&self.log_path, rotated_path)?;
        }

        Ok(())
    }
}

impl Default for SubmissionLogger {
    fn default() -> Self {
        Self::default_path()
    }
}

/// Statistics for submission logs
#[derive(Debug, Clone)]
pub struct LogStats {
    /// Total number of logs
    pub total: usize,
    /// Number of successful submissions
    pub successful: usize,
    /// Number of failed submissions
    pub failed: usize,
    /// Number of pending submissions
    pub pending: usize,
    /// Number of duplicate submissions
    pub duplicates: usize,
    /// Average duration in milliseconds
    pub avg_duration_ms: u64,
}

impl std::fmt::Display for LogStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {}, Successful: {}, Failed: {}, Pending: {}, Duplicates: {}, Avg Duration: {}ms",
            self.total, self.successful, self.failed, self.pending, self.duplicates, self.avg_duration_ms
        )
    }
}

/// In-memory submission tracker for duplicate detection
pub struct SubmissionTracker {
    /// Map of transaction hash to submission status
    submissions: Arc<Mutex<HashMap<String, TrackedSubmission>>>,
    /// Maximum number of submissions to track
    max_entries: usize,
}

#[derive(Debug, Clone)]
struct TrackedSubmission {
    status: SubmissionStatus,
    timestamp: SystemTime,
    request_id: String,
}

impl SubmissionTracker {
    /// Create a new submission tracker
    pub fn new() -> Self {
        Self {
            submissions: Arc::new(Mutex::new(HashMap::new())),
            max_entries: 10000,
        }
    }

    /// Create with custom max entries
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            submissions: Arc::new(Mutex::new(HashMap::new())),
            max_entries,
        }
    }

    /// Track a new submission
    pub fn track(&self, transaction_hash: impl Into<String>, request_id: impl Into<String>) {
        let mut submissions = self.submissions.lock().unwrap();

        // Remove oldest entries if at capacity
        if submissions.len() >= self.max_entries {
            let oldest = submissions
                .iter()
                .min_by_key(|(_, v)| v.timestamp)
                .map(|(k, _)| k.clone());
            if let Some(key) = oldest {
                submissions.remove(&key);
            }
        }

        submissions.insert(
            transaction_hash.into(),
            TrackedSubmission {
                status: SubmissionStatus::Pending,
                timestamp: SystemTime::now(),
                request_id: request_id.into(),
            },
        );
    }

    /// Update submission status
    pub fn update_status(
        &self,
        transaction_hash: &str,
        status: SubmissionStatus,
    ) -> Option<String> {
        let mut submissions = self.submissions.lock().unwrap();
        submissions.get_mut(transaction_hash).map(|sub| {
            sub.status = status;
            sub.request_id.clone()
        })
    }

    /// Check if a transaction is being tracked
    pub fn is_tracked(&self, transaction_hash: &str) -> bool {
        self.submissions.lock().unwrap().contains_key(transaction_hash)
    }

    /// Get the status of a tracked transaction
    pub fn get_status(&self, transaction_hash: &str) -> Option<SubmissionStatus> {
        self.submissions
            .lock()
            .unwrap()
            .get(transaction_hash)
            .map(|s| s.status)
    }

    /// Check if a transaction was successfully submitted
    pub fn is_successful(&self, transaction_hash: &str) -> bool {
        self.get_status(transaction_hash)
            .map(|s| s == SubmissionStatus::Success)
            .unwrap_or(false)
    }

    /// Remove a tracked submission
    pub fn remove(&self, transaction_hash: &str) {
        self.submissions.lock().unwrap().remove(transaction_hash);
    }

    /// Clear all tracked submissions
    pub fn clear(&self) {
        self.submissions.lock().unwrap().clear();
    }

    /// Get count of tracked submissions
    pub fn len(&self) -> usize {
        self.submissions.lock().unwrap().len()
    }

    /// Check if tracker is empty
    pub fn is_empty(&self) -> bool {
        self.submissions.lock().unwrap().is_empty()
    }
}

impl Default for SubmissionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_submission_log_creation() {
        let request = SubmissionRequest::new("test_xdr");
        let log = SubmissionLog::from_request(&request);

        assert_eq!(log.request_id, request.request_id);
        assert_eq!(log.status, "pending");
        assert!(log.transaction_hash.is_none());
    }

    #[test]
    fn test_submission_logger_memory() {
        let logger = SubmissionLogger::memory_only();

        let request = SubmissionRequest::new("test_xdr");
        let log = SubmissionLog::from_request(&request);

        logger.log_attempt(&log).unwrap();

        let recent = logger.get_recent_logs();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].request_id, request.request_id);
    }

    #[test]
    fn test_duplicate_detection() {
        let logger = SubmissionLogger::memory_only();

        let mut log = SubmissionLog::from_request(&SubmissionRequest::new("test"));
        log.transaction_hash = Some("hash123".to_string());
        log.status = "success".to_string();

        logger.log_attempt(&log).unwrap();

        assert!(logger.is_duplicate("hash123"));
        assert!(!logger.is_duplicate("hash456"));
    }

    #[test]
    fn test_submission_tracker() {
        let tracker = SubmissionTracker::new();

        tracker.track("tx_hash", "req_123");
        assert!(tracker.is_tracked("tx_hash"));
        assert!(!tracker.is_successful("tx_hash"));

        tracker.update_status("tx_hash", SubmissionStatus::Success);
        assert!(tracker.is_successful("tx_hash"));
    }

    #[test]
    fn test_log_stats() {
        let logger = SubmissionLogger::memory_only();

        // Add some test logs
        for i in 0..5 {
            let mut log = SubmissionLog::from_request(&SubmissionRequest::new("test"));
            log.status = if i < 3 { "success".to_string() } else { "failed".to_string() };
            log.duration_ms = 100 * (i as u64 + 1);
            logger.log_attempt(&log).unwrap();
        }

        let stats = logger.get_stats();
        assert_eq!(stats.total, 5);
        assert_eq!(stats.successful, 3);
        assert_eq!(stats.failed, 2);
        assert!(stats.avg_duration_ms > 0);
    }
}
