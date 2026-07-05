//! Profiler tab — trigger async-profiler recordings and view results.
//!
//! Shows recording status, event type selection, and opens flame graph
//! in the browser when complete.

use crate::collector::AppState;
use crate::profiler::{ProfileEvent, ProfileManager, ProfileStatus};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct ProfilerTab {
    pub state: Arc<AppState>,
    pub manager: ProfileManager,
    pub selected_event: usize,
    pub duration_secs: u64,
}

impl ProfilerTab {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            manager: ProfileManager::new(),
            selected_event: 0,
            duration_secs: 30,
        }
    }

    pub fn cycle_event(&mut self) {
        self.selected_event = (self.selected_event + 1) % 4;
        self.manager.config.event = match self.selected_event {
            0 => ProfileEvent::Cpu,
            1 => ProfileEvent::Alloc,
            2 => ProfileEvent::Wall,
            3 => ProfileEvent::Lock,
            _ => ProfileEvent::Cpu,
        };
    }

    pub fn increase_duration(&mut self) {
        self.duration_secs = (self.duration_secs + 10).min(300);
        self.manager.config.duration_secs = self.duration_secs;
    }

    pub fn decrease_duration(&mut self) {
        self.duration_secs = self.duration_secs.saturating_sub(10).max(5);
        self.manager.config.duration_secs = self.duration_secs;
    }

    /// Start a recording (Enter key). Respects --readonly.
    pub fn start_recording(&mut self) {
        if self.state.readonly {
            self.manager.status =
                ProfileStatus::Error("readonly mode — profiling disabled".to_string());
            return;
        }
        if self.manager.is_recording() {
            return;
        }
        if let Err(e) = self.manager.start_local() {
            self.manager.status = ProfileStatus::Error(e.to_string());
        }
    }

    /// Open the last completed flame graph in the browser ('o' key).
    pub fn open_result(&mut self) {
        if let Some(path) = self.manager.last_output.clone() {
            if let Err(e) = self.manager.open_in_browser(&path) {
                self.manager.status = ProfileStatus::Error(e.to_string());
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8), // Controls
                Constraint::Length(5), // Status
                Constraint::Min(3),    // Instructions / output
            ])
            .split(area);

        // Controls panel
        let events = ["CPU", "Alloc", "Wall", "Lock"];
        let event_line: Vec<Span> = events
            .iter()
            .enumerate()
            .map(|(i, name)| {
                if i == self.selected_event {
                    Span::styled(
                        format!(" [{}] ", name),
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
                    )
                } else {
                    Span::styled(format!("  {}  ", name), Style::default().fg(Color::White))
                }
            })
            .collect();

        let controls = vec![
            Line::from(""),
            Line::from(vec![Span::raw("  Event Type: "), Span::raw("")]),
            Line::from(event_line),
            Line::from(format!(
                "  Duration:   {}s  (+/- to adjust)",
                self.duration_secs
            )),
            Line::from("  Format:     HTML Flame Graph (opens in browser)".to_string()),
            Line::from(""),
        ];
        frame.render_widget(
            Paragraph::new(controls).block(
                Block::default()
                    .title(" Profiler Controls (e:event  +/-:duration  Enter:start) ")
                    .borders(Borders::ALL),
            ),
            chunks[0],
        );

        // Status panel
        let (status_text, status_color) = match &self.manager.status {
            ProfileStatus::Idle => (
                "Ready — press Enter to start recording".to_string(),
                Color::Green,
            ),
            ProfileStatus::Recording {
                event,
                elapsed_secs,
                total_secs,
            } => {
                let bar_width = 30;
                let filled =
                    (*elapsed_secs as f64 / *total_secs as f64 * bar_width as f64) as usize;
                let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
                (
                    format!(
                        "Recording {:?}... [{}] {}/{}s",
                        event, bar, elapsed_secs, total_secs
                    ),
                    Color::Yellow,
                )
            }
            ProfileStatus::Processing => ("Processing flame graph...".to_string(), Color::Cyan),
            ProfileStatus::Complete { output_path } => (
                format!(
                    "✓ Complete: {} — press 'o' to open in browser",
                    output_path.display()
                ),
                Color::Green,
            ),
            ProfileStatus::Error(msg) => (format!("✗ Error: {}", msg), Color::Red),
        };
        frame.render_widget(
            Paragraph::new(format!("  {}", status_text))
                .style(Style::default().fg(status_color))
                .block(Block::default().title(" Status ").borders(Borders::ALL)),
            chunks[1],
        );

        // Instructions
        let instructions = vec![
            Line::from(""),
            Line::from("  How profiling works:").style(Style::default().fg(Color::Cyan).bold()),
            Line::from(""),
            Line::from("  For REMOTE targets (via Jolokia):"),
            Line::from("    1. async-profiler must be loaded on the target JVM"),
            Line::from("       -agentpath:/path/to/libasyncProfiler.so"),
            Line::from("    2. drishti sends start/stop commands via Jolokia exec"),
            Line::from("    3. Output file is fetched and opened in your browser"),
            Line::from(""),
            Line::from("  For LOCAL targets (same machine):"),
            Line::from(
                "    1. Install async-profiler: https://github.com/async-profiler/async-profiler",
            ),
            Line::from("    2. drishti invokes 'asprof' CLI directly"),
            Line::from("    3. HTML flame graph opens automatically"),
            Line::from(""),
            Line::from("  Keys:  e=event type  +/-=duration  Enter=start  o=open result"),
        ];
        frame.render_widget(
            Paragraph::new(instructions).block(
                Block::default()
                    .title(" async-profiler Integration ")
                    .borders(Borders::ALL),
            ),
            chunks[2],
        );
    }
}
