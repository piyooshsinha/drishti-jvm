//! Configuration loading via figment — supports TOML files + environment variables.
//!
//! Load order (later overrides earlier):
//! 1. Compiled defaults
//! 2. /etc/drishti-jvm/config.toml
//! 3. ~/.config/drishti-jvm/config.toml
//! 4. ./drishti-jvm.toml (project-local)
//! 5. DRISHTI_ prefixed environment variables
//! 6. CLI arguments (override everything)

use figment::{Figment, providers::{Format, Toml, Env, Serialized}};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub target: TargetConfig,
    pub polling: PollingConfig,
    pub ui: UiConfig,
    pub alerts: AlertConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub mode: String,  // "actuator" | "jolokia" | "both"
    pub actuator_url: String,
    pub jolokia_url: String,
    pub gc_log_path: Option<String>,
    pub app_log_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    pub metrics_interval_secs: u64,
    pub gc_log_poll_ms: u64,
    pub thread_dump_interval_secs: u64,
    pub deadlock_check_interval_secs: u64,
    pub anomaly_eval_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub tick_rate_hz: u64,
    pub render_rate_hz: u64,
    pub chart_history_multiplier: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub suppress_duration_secs: u64,
    pub thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    pub memory_leak_warn_slope: f64,
    pub memory_leak_critical_slope: f64,
    pub gc_throughput_warn: f64,
    pub allocation_rate_warn: u64,
    pub heap_warn_pct: f64,
    pub heap_critical_pct: f64,
    pub hikaricp_pending_warn_secs: u64,
    pub hikaricp_pending_critical_secs: u64,
    pub http_error_rate_warn: f64,
    pub http_error_rate_critical: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target: TargetConfig {
                mode: "both".to_string(),
                actuator_url: "http://localhost:8080/actuator".to_string(),
                jolokia_url: "http://localhost:8778/jolokia".to_string(),
                gc_log_path: None,
                app_log_path: None,
            },
            polling: PollingConfig {
                metrics_interval_secs: 2,
                gc_log_poll_ms: 500,
                thread_dump_interval_secs: 10,
                deadlock_check_interval_secs: 15,
                anomaly_eval_interval_secs: 30,
            },
            ui: UiConfig {
                theme: "dark".to_string(),
                tick_rate_hz: 4,
                render_rate_hz: 30,
                chart_history_multiplier: 2,
            },
            alerts: AlertConfig {
                suppress_duration_secs: 300,
                thresholds: AlertThresholds {
                    memory_leak_warn_slope: 0.05,
                    memory_leak_critical_slope: 0.10,
                    gc_throughput_warn: 0.95,
                    allocation_rate_warn: 1_073_741_824, // 1 GB/s
                    heap_warn_pct: 80.0,
                    heap_critical_pct: 90.0,
                    hikaricp_pending_warn_secs: 30,
                    hikaricp_pending_critical_secs: 60,
                    http_error_rate_warn: 0.05,
                    http_error_rate_critical: 0.10,
                },
            },
        }
    }
}

impl Config {
    /// Load configuration from all sources.
    pub fn load() -> Result<Self, figment::Error> {
        let config: Config = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file("/etc/drishti-jvm/config.toml").nested())
            .merge(Toml::file(dirs_config_path()).nested())
            .merge(Toml::file("drishti-jvm.toml").nested())
            .merge(Env::prefixed("DRISHTI_").split("__"))
            .extract()?;
        Ok(config)
    }

    /// Apply CLI overrides onto the loaded config.
    pub fn with_cli_overrides(
        mut self,
        actuator: Option<&str>,
        jolokia: Option<&str>,
        gc_log: Option<&str>,
    ) -> Self {
        if let Some(url) = actuator { self.target.actuator_url = url.to_string(); }
        if let Some(url) = jolokia { self.target.jolokia_url = url.to_string(); }
        if let Some(path) = gc_log { self.target.gc_log_path = Some(path.to_string()); }
        self
    }
}

fn dirs_config_path() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        format!("{}/.config/drishti-jvm/config.toml", home.to_string_lossy())
    } else {
        "~/.config/drishti-jvm/config.toml".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = Config::default();
        assert_eq!(cfg.polling.metrics_interval_secs, 2);
        assert_eq!(cfg.alerts.thresholds.gc_throughput_warn, 0.95);
        assert_eq!(cfg.target.mode, "both");
    }
}
