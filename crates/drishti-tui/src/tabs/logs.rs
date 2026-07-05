//! Logs tab — live application log display with error rate tracking and log-level control.
//!
//! Shows a scrollable log buffer with color-coded severity, an error rate
//! counter, and the ability to change logger levels via Actuator POST.

use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;
use std::collections::VecDeque;

/// A single log entry with parsed severity.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub logger: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn color(&self) -> Color {
        match self {
            LogLevel::Trace => Color::DarkGray,
            LogLevel::Debug => Color::Cyan,
            LogLevel::Info => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => " INFO",
            LogLevel::Warn => " WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// Parse a Spring Boot log line into a LogEntry.
pub fn parse_log_line(line: &str) -> LogEntry {
    // Spring Boot default: 2024-01-15T10:30:00.123  INFO 12345 --- [main] c.e.App : message
    let level = if line.contains(" ERROR ") { LogLevel::Error }
        else if line.contains(" WARN ") { LogLevel::Warn }
        else if line.contains(" INFO ") { LogLevel::Info }
        else if line.contains(" DEBUG ") { LogLevel::Debug }
        else if line.contains(" TRACE ") { LogLevel::Trace }
        else { LogLevel::Info };

    let timestamp = line.get(..23).unwrap_or("").to_string();
    let message = line.to_string();

    LogEntry {
        timestamp,
        level,
        logger: String::new(),
        message,
    }
}

pub struct LogsTab {
    pub state: Arc<AppState>,
    pub log_buffer: Arc<std::sync::Mutex<VecDeque<LogEntry>>>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub error_count: u64,
    pub warn_count: u64,
    pub total_count: u64,
    pub min_level: LogLevel,
}

impl LogsTab {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            log_buffer: Arc::new(std::sync::Mutex::new(VecDeque::with_capacity(2000))),
            scroll_offset: 0,
            auto_scroll: true,
            error_count: 0,
            warn_count: 0,
            total_count: 0,
            min_level: LogLevel::Info,
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        self.total_count += 1;
        match entry.level {
            LogLevel::Error => self.error_count += 1,
            LogLevel::Warn => self.warn_count += 1,
            _ => {}
        }
        if let Ok(mut buf) = self.log_buffer.lock() {
            buf.push_back(entry);
            if buf.len() > 2000 { buf.pop_front(); }
        }
    }

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset += 1;
    }

    pub fn scroll_bottom(&mut self) {
        self.auto_scroll = true;
        self.scroll_offset = 0;
    }

    pub fn cycle_min_level(&mut self) {
        self.min_level = match self.min_level {
            LogLevel::Trace => LogLevel::Debug,
            LogLevel::Debug => LogLevel::Info,
            LogLevel::Info => LogLevel::Warn,
            LogLevel::Warn => LogLevel::Error,
            LogLevel::Error => LogLevel::Trace,
        };
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);

        // Header: error/warn counts + level filter
        let error_style = if self.error_count > 0 { Style::default().fg(Color::Red) } else { Style::default() };
        let warn_style = if self.warn_count > 0 { Style::default().fg(Color::Yellow) } else { Style::default() };
        let header_text = vec![
            Line::from(vec![
                Span::raw("  Total: "),
                Span::styled(format!("{}", self.total_count), Style::default().fg(Color::White)),
                Span::raw("  Errors: "),
                Span::styled(format!("{}", self.error_count), error_style),
                Span::raw("  Warns: "),
                Span::styled(format!("{}", self.warn_count), warn_style),
                Span::raw("  │  Filter: ≥"),
                Span::styled(self.min_level.label(), Style::default().fg(self.min_level.color()).bold()),
                Span::raw(" (L to cycle)"),
                Span::raw(if self.auto_scroll { "  │  ↓AUTO" } else { "" }),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(header_text).block(Block::default().title(" Logs ").borders(Borders::ALL)),
            chunks[0],
        );

        // Log lines
        let visible_height = chunks[1].height.saturating_sub(2) as usize;
        let block = Block::default()
            .title(" Log Output (j/k:scroll  G:bottom  L:level) ")
            .borders(Borders::ALL);

        if let Ok(buf) = self.log_buffer.lock() {
            // Filter by level
            let filtered: Vec<&LogEntry> = buf.iter()
                .filter(|e| (e.level as u8) >= (self.min_level as u8))
                .collect();

            let total_filtered = filtered.len();
            let offset = if self.auto_scroll {
                total_filtered.saturating_sub(visible_height)
            } else {
                self.scroll_offset.min(total_filtered.saturating_sub(visible_height))
            };

            let lines: Vec<Line> = filtered.iter()
                .skip(offset)
                .take(visible_height)
                .map(|entry| {
                    Line::from(vec![
                        Span::styled(
                            format!("[{}] ", entry.level.label()),
                            Style::default().fg(entry.level.color()).bold(),
                        ),
                        Span::styled(
                            entry.message.chars().take(area.width as usize - 10).collect::<String>(),
                            Style::default().fg(entry.level.color()),
                        ),
                    ])
                }).collect();

            frame.render_widget(Paragraph::new(lines).block(block), chunks[1]);
        } else {
            frame.render_widget(
                Paragraph::new("  Log buffer unavailable").block(block),
                chunks[1],
            );
        }
    }
}
