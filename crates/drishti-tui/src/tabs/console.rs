//! Console tab — command-line REPL for JVM diagnostics.
//!
//! Supports commands like:
//!   dashboard       — show key metrics summary
//!   threads         — list all threads
//!   thread <id>     — show thread stack trace
//!   gc              — show GC stats
//!   memory          — show memory pools
//!   heap            — show heap usage
//!   loglevel <logger> <LEVEL> — change log level via Actuator
//!   mbean <pattern> — search MBeans
//!   help            — show available commands
//!   clear           — clear output

use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Debug, Clone)]
struct ConsoleLine {
    text: String,
    style: Style,
}

pub struct ConsoleTab {
    pub state: Arc<AppState>,
    input: String,
    cursor_pos: usize,
    output: VecDeque<ConsoleLine>,
    history: Vec<String>,
    history_index: Option<usize>,
    output_scroll: usize,
}

impl ConsoleTab {
    pub fn new(state: Arc<AppState>) -> Self {
        let mut tab = Self {
            state,
            input: String::new(),
            cursor_pos: 0,
            output: VecDeque::with_capacity(500),
            history: Vec::new(),
            history_index: None,
            output_scroll: 0,
        };
        tab.push_output("दृष्टि drishti-jvm console", Style::default().fg(Color::Cyan).bold());
        tab.push_output("Type 'help' for available commands.", Style::default().fg(Color::DarkGray));
        tab.push_output("", Style::default());
        tab
    }

    fn push_output(&mut self, text: &str, style: Style) {
        self.output.push_back(ConsoleLine { text: text.to_string(), style });
        if self.output.len() > 500 { self.output.pop_front(); }
    }

    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
        self.history_index = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.input.remove(self.cursor_pos);
        }
    }

    pub fn cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.input.len() { self.cursor_pos += 1; }
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        let idx = match self.history_index {
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
            None => self.history.len() - 1,
        };
        self.history_index = Some(idx);
        self.input = self.history[idx].clone();
        self.cursor_pos = self.input.len();
    }

    pub fn history_down(&mut self) {
        if let Some(idx) = self.history_index {
            if idx < self.history.len() - 1 {
                self.history_index = Some(idx + 1);
                self.input = self.history[idx + 1].clone();
            } else {
                self.history_index = None;
                self.input.clear();
            }
            self.cursor_pos = self.input.len();
        }
    }

    pub fn execute(&mut self) {
        let cmd = self.input.trim().to_string();
        if cmd.is_empty() { return; }

        self.push_output(&format!("> {}", cmd), Style::default().fg(Color::Green));
        self.history.push(cmd.clone());
        self.history_index = None;
        self.input.clear();
        self.cursor_pos = 0;

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts.first().map(|s| *s) {
            Some("help") | Some("?") => self.cmd_help(),
            Some("dashboard") | Some("dash") => self.cmd_dashboard(),
            Some("threads") | Some("thread") => self.cmd_threads(parts.get(1).copied()),
            Some("gc") => self.cmd_gc(),
            Some("memory") | Some("mem") => self.cmd_memory(),
            Some("heap") => self.cmd_heap(),
            Some("clear") | Some("cls") => { self.output.clear(); }
            Some("uptime") => self.cmd_uptime(),
            Some("alerts") => self.cmd_alerts(),
            Some(other) => {
                self.push_output(&format!("Unknown command: '{}'. Type 'help' for commands.", other),
                    Style::default().fg(Color::Red));
            }
            None => {}
        }
    }

    fn cmd_help(&mut self) {
        let commands = vec![
            ("dashboard", "Key metrics overview"),
            ("threads [id]", "List threads or show stack trace for thread ID"),
            ("gc", "GC collector statistics"),
            ("memory", "Memory pool breakdown"),
            ("heap", "Heap usage summary"),
            ("uptime", "JVM uptime and version"),
            ("alerts", "Current anomaly alerts"),
            ("clear", "Clear console output"),
            ("help", "Show this help"),
        ];
        self.push_output("Available commands:", Style::default().fg(Color::Cyan).bold());
        for (cmd, desc) in commands {
            self.push_output(&format!("  {:20} {}", cmd, desc), Style::default());
        }
    }

    fn cmd_dashboard(&mut self) {
        let snap = self.state.current();
        let derived = self.state.current_derived();
        self.push_output("=== Dashboard ===", Style::default().fg(Color::Cyan).bold());
        let heap_pct = snap.heap.usage_pct().unwrap_or(0.0);
        self.push_output(&format!("  Heap: {:.0}M / {:.0}M ({:.1}%)", snap.heap.used_mb(), snap.heap.max_mb(), heap_pct),
            Style::default().fg(if heap_pct > 85.0 { Color::Red } else { Color::White }));
        self.push_output(&format!("  CPU:  Proc:{:.1}% Sys:{:.1}%", snap.cpu.process_cpu_pct(), snap.cpu.system_cpu_pct()), Style::default());
        self.push_output(&format!("  Threads: {} live, {} daemon", snap.thread_summary.live, snap.thread_summary.daemon), Style::default());
        self.push_output(&format!("  GC Throughput: {:.1}%", derived.gc_throughput * 100.0), Style::default());
        self.push_output(&format!("  HTTP: {:.1} req/s, {:.1}ms avg", derived.http_requests_per_sec, snap.http.avg_latency_ms), Style::default());
        if let Some(ref h) = snap.hikari {
            self.push_output(&format!("  HikariCP: {}/{} active, {} pending", h.active, h.max, h.pending), Style::default());
        }
    }

    fn cmd_threads(&mut self, id_str: Option<&str>) {
        let snap = self.state.current();
        if let Some(id_s) = id_str {
            if let Ok(id) = id_s.parse::<i64>() {
                if let Some(t) = snap.threads.iter().find(|t| t.id == id) {
                    self.push_output(&format!("Thread #{}: {} [{:?}]", t.id, t.name, t.state),
                        Style::default().fg(Color::Cyan));
                    self.push_output(&format!("  daemon={} blocked={} waited={}", t.daemon, t.blocked_count, t.waited_count), Style::default());
                    if let Some(ref lock) = t.lock_name {
                        self.push_output(&format!("  waiting on: {}", lock), Style::default().fg(Color::Yellow));
                    }
                    for frame in t.stack_frames.iter().take(15) {
                        self.push_output(&format!("    at {}", frame), Style::default().fg(Color::DarkGray));
                    }
                } else {
                    self.push_output(&format!("Thread {} not found", id), Style::default().fg(Color::Red));
                }
                return;
            }
        }

        self.push_output(&format!("=== Threads ({}) ===", snap.threads.len()), Style::default().fg(Color::Cyan).bold());
        for t in snap.threads.iter().take(30) {
            let color = match t.state {
                drishti_core::model::ThreadState::Runnable => Color::Green,
                drishti_core::model::ThreadState::Blocked => Color::Red,
                drishti_core::model::ThreadState::Waiting => Color::Yellow,
                _ => Color::White,
            };
            self.push_output(&format!("  {:>6} {:40} {:?}", t.id, t.name.chars().take(40).collect::<String>(), t.state),
                Style::default().fg(color));
        }
        if snap.threads.len() > 30 {
            self.push_output(&format!("  ... and {} more", snap.threads.len() - 30), Style::default().fg(Color::DarkGray));
        }
    }

    fn cmd_gc(&mut self) {
        let snap = self.state.current();
        self.push_output("=== GC Collectors ===", Style::default().fg(Color::Cyan).bold());
        for c in &snap.gc_collectors {
            let avg = if c.collection_count > 0 { c.collection_time_ms as f64 / c.collection_count as f64 } else { 0.0 };
            self.push_output(&format!("  {}: {} collections, {}ms total, {:.1}ms avg",
                c.name, c.collection_count, c.collection_time_ms, avg), Style::default());
        }
        let gc_events = self.state.get_gc_events();
        if !gc_events.is_empty() {
            self.push_output(&format!("\n=== Recent GC Events ({}) ===", gc_events.len()), Style::default().fg(Color::Cyan).bold());
            for ev in gc_events.iter().rev().take(10) {
                let color = if ev.phase == drishti_core::model::GcPhase::FullGc { Color::Red } else { Color::White };
                self.push_output(&format!("  GC({}) {:?} {} {:.1}ms {}M→{}M",
                    ev.id, ev.phase, ev.cause, ev.pause_ms,
                    ev.heap_before_bytes / 1_048_576, ev.heap_after_bytes / 1_048_576),
                    Style::default().fg(color));
            }
        }
    }

    fn cmd_memory(&mut self) {
        let snap = self.state.current();
        self.push_output("=== Memory Pools ===", Style::default().fg(Color::Cyan).bold());
        for p in &snap.memory_pools {
            let pct_str = p.usage.usage_pct()
                .map(|p| format!("{:.1}%", p))
                .unwrap_or_else(|| "N/A".to_string());
            self.push_output(&format!("  {:30} {:>8.1}M / {:>8.1}M  {}  {:?}",
                p.name, p.usage.used_mb(), p.usage.max_mb(), pct_str, p.pool_type), Style::default());
        }
    }

    fn cmd_heap(&mut self) {
        let snap = self.state.current();
        let pct = snap.heap.usage_pct().unwrap_or(0.0);
        self.push_output("=== Heap ===", Style::default().fg(Color::Cyan).bold());
        self.push_output(&format!("  Used:      {:.1} MB", snap.heap.used_mb()), Style::default());
        self.push_output(&format!("  Committed: {:.1} MB", snap.heap.committed as f64 / 1_048_576.0), Style::default());
        self.push_output(&format!("  Max:       {:.1} MB", snap.heap.max_mb()), Style::default());
        self.push_output(&format!("  Usage:     {:.1}%", pct),
            Style::default().fg(if pct > 85.0 { Color::Red } else { Color::Green }));
    }

    fn cmd_uptime(&mut self) {
        let snap = self.state.current();
        self.push_output("=== JVM Info ===", Style::default().fg(Color::Cyan).bold());
        self.push_output(&format!("  VM:      {} {}", snap.jvm_info.vm_name, snap.jvm_info.vm_version), Style::default());
        self.push_output(&format!("  Vendor:  {}", snap.jvm_info.vm_vendor), Style::default());
        self.push_output(&format!("  Java:    {}", snap.jvm_info.spec_version), Style::default());
        self.push_output(&format!("  Uptime:  {}", snap.jvm_info.uptime_human()), Style::default());
        self.push_output(&format!("  GC:      {}", snap.jvm_info.gc_algorithm_str()), Style::default());
    }

    fn cmd_alerts(&mut self) {
        let snap = self.state.current();
        let history = self.state.get_history();
        let engine = drishti_core::detectors::default_engine();
        let alerts = engine.evaluate(&snap, &history);
        if alerts.is_empty() {
            self.push_output("✓ No active alerts", Style::default().fg(Color::Green));
        } else {
            self.push_output(&format!("=== Alerts ({}) ===", alerts.len()), Style::default().fg(Color::Cyan).bold());
            for a in &alerts {
                let color = match a.severity {
                    drishti_core::anomaly::Severity::Critical => Color::Red,
                    drishti_core::anomaly::Severity::High => Color::LightRed,
                    drishti_core::anomaly::Severity::Warn => Color::Yellow,
                    drishti_core::anomaly::Severity::Info => Color::Cyan,
                };
                self.push_output(&format!("  [{}] {} — {}", a.severity, a.title, a.detail),
                    Style::default().fg(color));
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .split(area);

        // Output area
        let visible = chunks[0].height.saturating_sub(2) as usize;
        let total = self.output.len();
        let skip = total.saturating_sub(visible);

        let lines: Vec<Line> = self.output.iter()
            .skip(skip)
            .take(visible)
            .map(|l| Line::from(l.text.clone()).style(l.style))
            .collect();

        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().title(" Console Output ").borders(Borders::ALL)),
            chunks[0],
        );

        // Input line
        let input_display = format!("drishti> {}", self.input);
        let input_block = Block::default().title(" Command ").borders(Borders::ALL);
        frame.render_widget(
            Paragraph::new(input_display).block(input_block),
            chunks[1],
        );

        // Show cursor
        let cursor_x = chunks[1].x + 10 + self.cursor_pos as u16;
        let cursor_y = chunks[1].y + 1;
        if cursor_x < chunks[1].right() {
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
    }
}
