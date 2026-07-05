mod action;
mod app;
mod collector;
mod config;
mod profiler;
mod components;
mod tabs;

use app::App;
use action::Action;
use collector::{AppState, spawn_collectors};
use color_eyre::eyre::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::stderr;
use std::sync::Arc;
use clap::Parser;
use tokio_util::sync::CancellationToken;

#[derive(Parser, Debug)]
#[command(name = "drishti-jvm", version, about = "दृष्टि — JVM/Spring Boot diagnostic TUI")]
struct Cli {
    /// Spring Boot Actuator base URL
    #[arg(long, default_value = "http://localhost:8080/actuator")]
    actuator: String,

    /// Jolokia agent URL
    #[arg(long, default_value = "http://localhost:8778/jolokia")]
    jolokia: String,

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    color_eyre::install()?;

    let _cfg = config::Config::load().unwrap_or_default();

    let state = Arc::new(AppState::new(cli.readonly));
    let cancel = CancellationToken::new();

    let actuator_url = if cli.no_actuator { None } else { Some(cli.actuator) };
    let jolokia_url = if cli.no_jolokia { None } else { Some(cli.jolokia) };

    let channels = spawn_collectors(state.clone(), actuator_url, jolokia_url, cli.gc_log, cancel.clone());

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
