mod action;
mod app;
mod collector;
mod components;
mod config;
mod profiler;
mod tabs;

use action::Action;
use app::App;
use clap::Parser;
use collector::{spawn_collectors, AppState, CollectorConfig};
use color_eyre::eyre::{eyre, Result};
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::stderr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Parser, Debug)]
#[command(
    name = "drishti-jvm",
    version,
    about = "दृष्टि — JVM/Spring Boot diagnostic TUI"
)]
struct Cli {
    /// Spring Boot Actuator base URL (overrides config file)
    #[arg(long)]
    actuator: Option<String>,

    /// Jolokia agent URL (overrides config file)
    #[arg(long)]
    jolokia: Option<String>,

    /// Path to GC log file (for local tailing)
    #[arg(long)]
    gc_log: Option<String>,

    /// Read-only mode (disables log-level changes, heap dumps)
    #[arg(long, default_value_t = false)]
    readonly: bool,

    /// Disable Actuator connection
    #[arg(long, default_value_t = false)]
    no_actuator: bool,

    /// Disable Jolokia connection
    #[arg(long, default_value_t = false)]
    no_jolokia: bool,

    /// Headless: take a single snapshot, print it, and exit
    #[arg(long, default_value_t = false)]
    once: bool,

    /// With --once: print the snapshot as JSON (for scripting)
    #[arg(long, default_value_t = false)]
    json: bool,

    /// With --once: print anomaly alerts and tuning recommendations
    #[arg(long, default_value_t = false)]
    recommendations: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    color_eyre::install()?;

    let cfg = config::Config::load()
        .unwrap_or_default()
        .with_cli_overrides(
            cli.actuator.as_deref(),
            cli.jolokia.as_deref(),
            cli.gc_log.as_deref(),
        );

    let actuator_url = if cli.no_actuator || cfg.target.mode == "jolokia" {
        None
    } else {
        Some(cfg.target.actuator_url.clone())
    };
    let jolokia_url = if cli.no_jolokia || cfg.target.mode == "actuator" {
        None
    } else {
        Some(cfg.target.jolokia_url.clone())
    };

    if cli.once {
        return run_once(actuator_url, jolokia_url, cli.json, cli.recommendations).await;
    }

    let state = Arc::new(AppState::new(cli.readonly));
    let cancel = CancellationToken::new();

    let channels = spawn_collectors(
        state.clone(),
        CollectorConfig {
            actuator_url,
            jolokia_url,
            gc_log_path: cfg.target.gc_log_path.clone(),
            metrics_interval: Duration::from_secs(cfg.polling.metrics_interval_secs.max(1)),
            thread_dump_interval: Duration::from_secs(cfg.polling.thread_dump_interval_secs.max(1)),
        },
        cancel.clone(),
    );

    enable_raw_mode()?;
    execute!(stderr(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stderr()))?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let mut app = App::new(state, channels);
    let tick = std::time::Duration::from_millis(100);
    while app.running {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let action = app.handle_key(key);
                    app.update(&action);
                }
            }
        }
        app.update(&Action::Tick);
    }

    cancel.cancel();
    disable_raw_mode()?;
    execute!(stderr(), LeaveAlternateScreen)?;
    Ok(())
}

/// Headless mode: fetch one merged snapshot and print it.
async fn run_once(
    actuator_url: Option<String>,
    jolokia_url: Option<String>,
    json: bool,
    recommendations: bool,
) -> Result<()> {
    use drishti_actuator::client::ActuatorAuth;
    use drishti_actuator::converter::prometheus_to_snapshot;
    use drishti_actuator::ActuatorClient;
    use drishti_core::model::JvmSnapshot;
    use drishti_jolokia::client::JolokiaAuth;
    use drishti_jolokia::converter::bulk_to_snapshot;
    use drishti_jolokia::JolokiaClient;

    let timeout = Duration::from_secs(10);
    let mut snap: Option<JvmSnapshot> = None;
    let mut errors: Vec<String> = Vec::new();

    if let Some(url) = &actuator_url {
        let client = ActuatorClient::new(url, ActuatorAuth::None, timeout);
        match client.scrape_prometheus_raw().await {
            Ok(text) => snap = Some(prometheus_to_snapshot(&text)),
            Err(e) => errors.push(format!("actuator: {e}")),
        }
    }
    if let Some(url) = &jolokia_url {
        let client = JolokiaClient::new(url, JolokiaAuth::None, timeout);
        match client.fetch_standard().await {
            Ok(responses) => {
                let j = bulk_to_snapshot(&responses);
                snap = Some(match snap {
                    Some(a) => collector::merge_snapshots(&a, &j),
                    None => j,
                });
            }
            Err(e) => errors.push(format!("jolokia: {e}")),
        }
    }

    let snap = snap.ok_or_else(|| {
        eyre!(
            "no data source reachable ({})",
            if errors.is_empty() {
                "no sources configured".to_string()
            } else {
                errors.join("; ")
            }
        )
    })?;

    for e in &errors {
        eprintln!("warning: {e}");
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }

    if recommendations {
        let history = vec![snap.clone()];
        let alerts = drishti_core::detectors::default_engine().evaluate(&snap, &history);
        let recs = drishti_core::rules::default_engine().evaluate(&snap, &history);

        println!("Alerts ({}):", alerts.len());
        for a in &alerts {
            println!(
                "  [{}] {} — {} (confidence {:.0}%)",
                a.severity,
                a.title,
                a.detail,
                a.confidence * 100.0
            );
        }
        println!("\nTuning recommendations ({}):", recs.len());
        for r in &recs {
            println!("  {} — {}", r.title, r.suggestion);
            if !r.jvm_flags.is_empty() {
                println!("    flags: {}", r.jvm_flags.join(" "));
            }
        }
        return Ok(());
    }

    // Default: a compact human-readable summary
    println!(
        "{} (uptime {})",
        snap.jvm_info.vm_name,
        snap.jvm_info.uptime_human()
    );
    println!(
        "heap: {:.0}M / {:.0}M ({:.0}%)   threads: {} live   cpu: {:.0}%",
        snap.heap.used_mb(),
        snap.heap.max_mb(),
        snap.heap.usage_pct().unwrap_or(0.0),
        snap.thread_summary.live,
        snap.cpu.process_cpu_pct(),
    );
    for c in &snap.gc_collectors {
        println!(
            "gc {}: {} collections, {}ms total",
            c.name, c.collection_count, c.collection_time_ms
        );
    }
    Ok(())
}
