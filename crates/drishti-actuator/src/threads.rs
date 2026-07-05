//! Thread dump parser for Spring Boot Actuator's `/actuator/threaddump` endpoint.

use drishti_core::model::{ThreadInfo, ThreadState};
use serde::Deserialize;

/// Raw response from `/actuator/threaddump`.
#[derive(Debug, Deserialize)]
pub struct ThreadDumpResponse {
    pub threads: Vec<RawThread>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThread {
    pub thread_id: i64,
    pub thread_name: String,
    pub thread_state: String,
    #[serde(default)]
    pub daemon: bool,
    #[serde(default)]
    pub blocked_count: i64,
    #[serde(default)]
    pub waited_count: i64,
    #[serde(default)]
    pub in_native: bool,
    #[serde(default)]
    pub suspended: bool,
    #[serde(default)]
    pub lock_name: Option<String>,
    #[serde(default)]
    pub lock_owner_name: Option<String>,
    #[serde(default)]
    pub lock_owner_id: Option<i64>,
    #[serde(default)]
    pub stack_trace: Vec<RawStackFrame>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawStackFrame {
    #[serde(default)]
    pub class_name: String,
    #[serde(default)]
    pub method_name: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub line_number: i32,
    #[serde(default)]
    pub native_method: bool,
}

impl RawStackFrame {
    pub fn to_string_frame(&self) -> String {
        let file = self.file_name.as_deref().unwrap_or("Unknown");
        let line = if self.line_number > 0 {
            format!(":{}", self.line_number)
        } else if self.native_method {
            "(Native Method)".to_string()
        } else {
            String::new()
        };
        format!("{}.{}({}{})", self.class_name, self.method_name, file, line)
    }
}

fn parse_thread_state(s: &str) -> ThreadState {
    match s.to_uppercase().replace('-', "_").as_str() {
        "NEW" => ThreadState::New,
        "RUNNABLE" => ThreadState::Runnable,
        "BLOCKED" => ThreadState::Blocked,
        "WAITING" => ThreadState::Waiting,
        "TIMED_WAITING" => ThreadState::TimedWaiting,
        "TERMINATED" => ThreadState::Terminated,
        _ => ThreadState::Unknown,
    }
}

/// Parse thread dump JSON into a Vec<ThreadInfo>.
pub fn parse_thread_dump(json: &str) -> Result<Vec<ThreadInfo>, serde_json::Error> {
    let response: ThreadDumpResponse = serde_json::from_str(json)?;

    Ok(response
        .threads
        .into_iter()
        .map(|t| ThreadInfo {
            id: t.thread_id,
            name: t.thread_name,
            state: parse_thread_state(&t.thread_state),
            daemon: t.daemon,
            cpu_time_ns: None,
            blocked_count: t.blocked_count,
            waited_count: t.waited_count,
            lock_name: t.lock_name,
            lock_owner_name: t.lock_owner_name,
            lock_owner_id: t.lock_owner_id,
            in_native: t.in_native,
            suspended: t.suspended,
            stack_frames: t
                .stack_trace
                .iter()
                .take(20)
                .map(|f| f.to_string_frame())
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_thread_dump() {
        let json = r#"{
            "threads": [
                {
                    "threadId": 1,
                    "threadName": "main",
                    "threadState": "RUNNABLE",
                    "daemon": false,
                    "blockedCount": 5,
                    "waitedCount": 10,
                    "stackTrace": [
                        {"className": "com.example.App", "methodName": "run", "fileName": "App.java", "lineNumber": 42, "nativeMethod": false}
                    ]
                },
                {
                    "threadId": 15,
                    "threadName": "HikariPool-1-connection-adder",
                    "threadState": "TIMED_WAITING",
                    "daemon": true,
                    "blockedCount": 0,
                    "waitedCount": 100,
                    "stackTrace": []
                }
            ]
        }"#;

        let threads = parse_thread_dump(json).unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].name, "main");
        assert_eq!(threads[0].state, ThreadState::Runnable);
        assert!(!threads[0].daemon);
        assert_eq!(threads[0].stack_frames.len(), 1);
        assert!(threads[0].stack_frames[0].contains("App.run"));
        assert_eq!(threads[1].state, ThreadState::TimedWaiting);
        assert!(threads[1].daemon);
    }
}
