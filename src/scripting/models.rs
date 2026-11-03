//! Data models for the LEAP Scripting & Automation Subsystem.

use chrono::Local;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptStatus {
    Idle,
    Running,
    Paused,
    Succeeded,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Pass,
    Fail,
    Warn,
    Leap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptLogMessage {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

impl ScriptLogMessage {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Local::now().format("%H:%M:%S%.3f").to_string(),
            level,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionRecord {
    pub passed: bool,
    pub description: String,
    pub actual: String,
    pub expected: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestReport {
    pub script_name: String,
    pub start_time: String,
    pub duration_ms: u64,
    pub total_assertions: usize,
    pub passed_assertions: usize,
    pub failed_assertions: usize,
    pub assertions: Vec<AssertionRecord>,
    pub logs: Vec<ScriptLogMessage>,
}

impl TestReport {
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!("# Automated LEAP Test Report: {}\n\n", self.script_name));
        md.push_str(&format!("* **Execution Timestamp:** {}\n", self.start_time));
        md.push_str(&format!("* **Duration:** {} ms ({:.2} s)\n", self.duration_ms, self.duration_ms as f64 / 1000.0));
        let status_emoji = if self.failed_assertions == 0 { "✅ PASSED" } else { "❌ FAILED" };
        md.push_str(&format!("* **Overall Result:** {}\n", status_emoji));
        md.push_str(&format!("* **Assertions:** {} Total | {} Passed | {} Failed\n\n",
            self.total_assertions, self.passed_assertions, self.failed_assertions));

        md.push_str("## Assertions Details\n\n");
        if self.assertions.is_empty() {
            md.push_str("*(No assertions executed)*\n\n");
        } else {
            md.push_str("| Status | Description | Actual | Expected |\n");
            md.push_str("| :---: | :--- | :--- | :--- |\n");
            for a in &self.assertions {
                let icon = if a.passed { "✅ Pass" } else { "❌ Fail" };
                md.push_str(&format!("| {} | {} | `{}` | `{}` |\n",
                    icon, a.description, a.actual, a.expected));
            }
            md.push_str("\n");
        }

        md.push_str("## Execution Logs\n\n```text\n");
        for log in &self.logs {
            let tag = match log.level {
                LogLevel::Info => "INFO",
                LogLevel::Pass => "PASS",
                LogLevel::Fail => "FAIL",
                LogLevel::Warn => "WARN",
                LogLevel::Leap => "LEAP",
            };
            md.push_str(&format!("[{}] [{}] {}\n", log.timestamp, tag, log.message));
        }
        md.push_str("```\n");

        md
    }
}
