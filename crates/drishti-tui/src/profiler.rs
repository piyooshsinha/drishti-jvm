//! Async-profiler integration for CPU/allocation flame graphs.
//!
//! Architecture:
//! 1. Trigger async-profiler via Jolokia exec on the target JVM
//!    (requires async-profiler loaded as -agentpath or attached)
//! 2. Wait for recording to complete
//! 3. Fetch the output file via Jolokia or /actuator/logfile
//! 4. Convert to HTML flame graph (using the profiler's built-in converter)
//! 5. Write to temp file and open in the user's browser
//!
//! Alternative: shell out to `asprof` CLI if available locally and
//! the target is on the same machine.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfilerError {
    #[error("async-profiler not available on target JVM")]
    NotAvailable,

    #[error("Profiling already in progress")]
    AlreadyRunning,

    #[error("Profile recording failed: {0}")]
    RecordingFailed(String),

    #[error("Failed to open browser: {0}")]
    BrowserFailed(String),

    #[error("Jolokia error: {0}")]
    JolokiaError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Type of profiling event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileEvent {
    /// CPU time profiling — where is time spent?
    Cpu,
    /// Allocation profiling — what allocates the most?
    Alloc,
    /// Wall-clock profiling — includes blocked/waiting time.
    Wall,
    /// Lock contention profiling.
    Lock,
}

impl ProfileEvent {
    pub fn as_str(&self) -> &str {
        match self {
            ProfileEvent::Cpu => "cpu",
            ProfileEvent::Alloc => "alloc",
            ProfileEvent::Wall => "wall",
            ProfileEvent::Lock => "lock",
        }
    }
}

/// Status of a profiling session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileStatus {
    Idle,
    Recording {
        event: ProfileEvent,
        elapsed_secs: u64,
        total_secs: u64,
    },
    Processing,
    Complete {
        output_path: PathBuf,
    },
    Error(String),
}

/// Configuration for a profiling session.
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub event: ProfileEvent,
    pub duration_secs: u64,
    pub output_format: OutputFormat,
    /// Target JVM PID (for local asprof invocation).
    pub target_pid: Option<u32>,
    /// Output directory for flame graph files.
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// HTML flame graph (self-contained, interactive).
    Html,
    /// Collapsed stacks (for further processing).
    Collapsed,
    /// JFR format (for JDK Mission Control).
    Jfr,
}

impl OutputFormat {
    pub fn extension(&self) -> &str {
        match self {
            OutputFormat::Html => "html",
            OutputFormat::Collapsed => "collapsed",
            OutputFormat::Jfr => "jfr",
        }
    }
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            event: ProfileEvent::Cpu,
            duration_secs: 30,
            output_format: OutputFormat::Html,
            target_pid: None,
            output_dir: std::env::temp_dir().join("drishti-profiles"),
        }
    }
}

/// Manages profiling sessions.
pub struct ProfileManager {
    pub status: ProfileStatus,
    pub config: ProfileConfig,
    pub last_output: Option<PathBuf>,
}

impl ProfileManager {
    pub fn new() -> Self {
        Self {
            status: ProfileStatus::Idle,
            config: ProfileConfig::default(),
            last_output: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.status, ProfileStatus::Recording { .. })
    }

    /// Build the async-profiler command string for Jolokia exec.
    ///
    /// This is sent via:
    /// POST /jolokia { "type":"exec", "mbean":"one.profiler:type=AsyncProfiler",
    ///   "operation":"execute", "arguments":["start,event=cpu,file=/tmp/profile.html"] }
    pub fn build_start_command(&self) -> String {
        let output_file = self.output_path();
        format!(
            "start,event={},file={},flamegraph",
            self.config.event.as_str(),
            output_file.display(),
        )
    }

    pub fn build_stop_command(&self) -> String {
        let output_file = self.output_path();
        format!("stop,file={}", output_file.display(),)
    }

    /// Build the asprof CLI command for local profiling.
    pub fn build_asprof_command(&self, pid: u32) -> Vec<String> {
        let output_file = self.output_path();
        vec![
            "asprof".to_string(),
            "-d".to_string(),
            self.config.duration_secs.to_string(),
            "-e".to_string(),
            self.config.event.as_str().to_string(),
            "-f".to_string(),
            output_file.display().to_string(),
            pid.to_string(),
        ]
    }

    fn output_path(&self) -> PathBuf {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        self.config.output_dir.join(format!(
            "drishti_{}_{}.{}",
            self.config.event.as_str(),
            timestamp,
            self.config.output_format.extension(),
        ))
    }

    /// Open the flame graph in the user's default browser.
    pub fn open_in_browser(&self, path: &Path) -> Result<(), ProfilerError> {
        let url = format!("file://{}", path.display());

        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&url)
                .spawn()
                .map_err(|e| ProfilerError::BrowserFailed(e.to_string()))?;
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&url)
                .spawn()
                .map_err(|e| ProfilerError::BrowserFailed(e.to_string()))?;
        }

        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn()
                .map_err(|e| ProfilerError::BrowserFailed(e.to_string()))?;
        }

        Ok(())
    }

    /// Start a local asprof profiling session. Returns the output path.
    ///
    /// The child process runs for `duration_secs` and writes the flame graph
    /// to the output path on its own; we don't block on it.
    pub fn start_local(&mut self) -> Result<PathBuf, ProfilerError> {
        if self.is_recording() {
            return Err(ProfilerError::AlreadyRunning);
        }

        let pid = self.config.target_pid.ok_or(ProfilerError::NotAvailable)?;

        // Ensure output directory exists
        std::fs::create_dir_all(&self.config.output_dir)?;

        let output_path = self.output_path();
        let args = self.build_asprof_command(pid);
        std::process::Command::new(&args[0])
            .args(&args[1..])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| ProfilerError::RecordingFailed(e.to_string()))?;

        self.status = ProfileStatus::Recording {
            event: self.config.event,
            elapsed_secs: 0,
            total_secs: self.config.duration_secs,
        };
        self.last_output = Some(output_path.clone());
        Ok(output_path)
    }

    /// Build the Jolokia request body for starting a remote profile.
    pub fn jolokia_start_request(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "exec",
            "mbean": "one.profiler:type=AsyncProfiler",
            "operation": "execute",
            "arguments": [self.build_start_command()]
        })
    }

    /// Build the Jolokia request body for stopping a remote profile.
    pub fn jolokia_stop_request(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "exec",
            "mbean": "one.profiler:type=AsyncProfiler",
            "operation": "execute",
            "arguments": [self.build_stop_command()]
        })
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a simple top-down tree view from collapsed stack data.
/// This is the TUI-friendly alternative to a graphical flame graph.
///
/// Input format (collapsed stacks):
/// ```text
/// com.example.App.main;com.example.Service.process;java.util.HashMap.get 42
/// com.example.App.main;com.example.Service.process;java.util.HashMap.put 18
/// ```
///
/// Output: a vector of (depth, frame_name, self_samples, total_samples).
pub fn collapsed_to_tree(collapsed: &str) -> Vec<TreeNode> {
    let mut root = TreeNode {
        name: "(all)".to_string(),
        self_samples: 0,
        total_samples: 0,
        children: Vec::new(),
    };

    for line in collapsed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (stack_str, count_str) = match line.rsplit_once(' ') {
            Some((s, c)) => (s, c),
            None => continue,
        };
        let count: u64 = match count_str.parse() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let frames: Vec<&str> = stack_str.split(';').collect();
        let mut current = &mut root;
        current.total_samples += count;

        for (i, frame) in frames.iter().enumerate() {
            let is_leaf = i == frames.len() - 1;

            let child_idx = current.children.iter().position(|c| c.name == *frame);
            let idx = match child_idx {
                Some(i) => i,
                None => {
                    current.children.push(TreeNode {
                        name: frame.to_string(),
                        self_samples: 0,
                        total_samples: 0,
                        children: Vec::new(),
                    });
                    current.children.len() - 1
                }
            };

            current = &mut current.children[idx];
            current.total_samples += count;
            if is_leaf {
                current.self_samples += count;
            }
        }
    }

    // Sort children by total_samples descending at each level
    sort_tree(&mut root);

    vec![root]
}

fn sort_tree(node: &mut TreeNode) {
    node.children
        .sort_by_key(|c| std::cmp::Reverse(c.total_samples));
    for child in &mut node.children {
        sort_tree(child);
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub self_samples: u64,
    pub total_samples: u64,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Flatten the tree into indented lines for TUI rendering.
    pub fn flatten(&self, depth: usize, total_root: u64) -> Vec<FlatProfileLine> {
        let mut lines = Vec::new();
        let pct = if total_root > 0 {
            self.total_samples as f64 / total_root as f64 * 100.0
        } else {
            0.0
        };
        let self_pct = if total_root > 0 {
            self.self_samples as f64 / total_root as f64 * 100.0
        } else {
            0.0
        };

        lines.push(FlatProfileLine {
            depth,
            name: self.name.clone(),
            total_pct: pct,
            self_pct,
            total_samples: self.total_samples,
            self_samples: self.self_samples,
        });

        for child in &self.children {
            lines.extend(child.flatten(depth + 1, total_root));
        }

        lines
    }
}

#[derive(Debug, Clone)]
pub struct FlatProfileLine {
    pub depth: usize,
    pub name: String,
    pub total_pct: f64,
    pub self_pct: f64,
    pub total_samples: u64,
    pub self_samples: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_collapsed_stacks() {
        let collapsed = "main;process;HashMap.get 42\nmain;process;HashMap.put 18\nmain;init 5\n";
        let tree = collapsed_to_tree(collapsed);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].total_samples, 65);
        assert_eq!(tree[0].children.len(), 1); // "main"
        assert_eq!(tree[0].children[0].name, "main");
        assert_eq!(tree[0].children[0].total_samples, 65);
    }

    #[test]
    fn flatten_tree() {
        let collapsed = "A;B;C 100\nA;B;D 50\nA;E 30\n";
        let tree = collapsed_to_tree(collapsed);
        let lines = tree[0].flatten(0, 180);
        assert!(lines.len() >= 5); // root, A, B, C, D, E
        assert!((lines[0].total_pct - 100.0).abs() < 0.1);
    }

    #[test]
    fn profile_config_defaults() {
        let cfg = ProfileConfig::default();
        assert_eq!(cfg.event, ProfileEvent::Cpu);
        assert_eq!(cfg.duration_secs, 30);
        assert_eq!(cfg.output_format, OutputFormat::Html);
    }

    #[test]
    fn build_asprof_command() {
        let mut mgr = ProfileManager::new();
        mgr.config.target_pid = Some(12345);
        let cmd = mgr.build_asprof_command(12345);
        assert_eq!(cmd[0], "asprof");
        assert!(cmd.contains(&"cpu".to_string()));
        assert!(cmd.contains(&"12345".to_string()));
    }
}
