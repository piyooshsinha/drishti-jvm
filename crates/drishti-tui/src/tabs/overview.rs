use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct OverviewTab {
    pub state: Arc<AppState>,
}

impl OverviewTab {
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let derived = self.state.current_derived();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Min(3),
            ])
            .split(area);

        // Row 1: Heap | CPU | Threads
        let r1 = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(rows[0]);

        let heap_pct = snap.heap.usage_pct().unwrap_or(0.0);
        frame.render_widget(
            Gauge::default()
                .block(
                    Block::default()
                        .title(" Heap ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(sev_color(heap_pct))),
                )
                .gauge_style(Style::default().fg(sev_color(heap_pct)))
                .percent(heap_pct.min(100.0) as u16)
                .label(format!(
                    "{:.0}M / {:.0}M ({:.0}%)",
                    snap.heap.used_mb(),
                    snap.heap.max_mb(),
                    heap_pct
                )),
            r1[0],
        );

        let cpu_pct = snap.cpu.process_cpu_pct();
        frame.render_widget(
            Gauge::default()
                .block(
                    Block::default()
                        .title(" Process CPU ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(sev_color(cpu_pct))),
                )
                .gauge_style(Style::default().fg(sev_color(cpu_pct)))
                .percent(cpu_pct.min(100.0) as u16)
                .label(format!(
                    "Proc:{:.0}% Sys:{:.0}% Cores:{}",
                    cpu_pct,
                    snap.cpu.system_cpu_pct(),
                    snap.cpu.available_processors
                )),
            r1[1],
        );

        let deadlock_str = if snap.deadlocks.is_empty() {
            ""
        } else {
            " ⚠DEADLOCK"
        };
        let thread_block = Block::default().title(" Threads ").borders(Borders::ALL);
        let dl_style = if snap.deadlocks.is_empty() {
            Style::default()
        } else {
            Style::default().fg(Color::Red)
        };
        let ts = &snap.thread_summary;
        let thread_text = vec![
            Line::from(format!(
                "  Live:{} Daemon:{} Peak:{}{}",
                ts.live, ts.daemon, ts.peak, deadlock_str
            )),
            Line::from(format!(
                "  RUN:{} WAIT:{} BLOCK:{} TW:{}",
                ts.state_counts
                    .get(&drishti_core::model::ThreadState::Runnable)
                    .unwrap_or(&0),
                ts.state_counts
                    .get(&drishti_core::model::ThreadState::Waiting)
                    .unwrap_or(&0),
                ts.state_counts
                    .get(&drishti_core::model::ThreadState::Blocked)
                    .unwrap_or(&0),
                ts.state_counts
                    .get(&drishti_core::model::ThreadState::TimedWaiting)
                    .unwrap_or(&0)
            )),
        ];
        frame.render_widget(
            Paragraph::new(thread_text)
                .block(thread_block)
                .style(dl_style),
            r1[2],
        );

        // Row 2: GC + Derived | HTTP | HikariCP
        let r2 = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(rows[1]);

        // GC with throughput
        let gc_block = Block::default().title(" GC ").borders(Borders::ALL);
        let throughput_pct = derived.gc_throughput * 100.0;
        let tp_color = if throughput_pct < 95.0 {
            Color::Red
        } else if throughput_pct < 98.0 {
            Color::Yellow
        } else {
            Color::Green
        };
        let mut gc_lines = vec![Line::from(format!(
            "  Throughput: {:.1}%  Overhead: {:.2}%",
            throughput_pct,
            derived.gc_overhead * 100.0
        ))
        .style(Style::default().fg(tp_color))];
        for c in snap.gc_collectors.iter().take(2) {
            let avg = if c.collection_count > 0 {
                c.collection_time_ms as f64 / c.collection_count as f64
            } else {
                0.0
            };
            gc_lines.push(Line::from(format!(
                "  {}: {} ({:.1}ms avg)",
                c.name, c.collection_count, avg
            )));
        }
        frame.render_widget(Paragraph::new(gc_lines).block(gc_block), r2[0]);

        // HTTP with RPS
        let http_block = Block::default().title(" HTTP ").borders(Borders::ALL);
        let http_text = vec![
            Line::from(format!(
                "  {:.1} req/s  Errors: {}",
                derived.http_requests_per_sec, snap.http.total_errors
            ))
            .style(if snap.http.total_errors > 0 {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            }),
            Line::from(format!(
                "  Avg latency: {:.1}ms  Endpoints: {}",
                snap.http.avg_latency_ms,
                snap.http.endpoints.len()
            )),
        ];
        frame.render_widget(Paragraph::new(http_text).block(http_block), r2[1]);

        // HikariCP
        let pool_block = Block::default()
            .title(" Connection Pool ")
            .borders(Borders::ALL);
        let pool_text = if let Some(ref h) = snap.hikari {
            let util = h.utilization_pct();
            vec![
                Line::from(format!(
                    "  Active:{}/{} Idle:{} Pending:{}",
                    h.active, h.max, h.idle, h.pending
                )),
                Line::from(format!("  Util:{:.0}% Timeouts:{}", util, h.timeout_count)).style(
                    if h.is_saturated() {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default()
                    },
                ),
            ]
        } else {
            vec![Line::from("  No HikariCP data")]
        };
        frame.render_widget(Paragraph::new(pool_text).block(pool_block), r2[2]);

        // Row 3: Alerts
        let alert_block = Block::default().title(" Alerts ").borders(Borders::ALL);
        let history = self.state.get_history();
        let engine = drishti_core::detectors::default_engine();
        let alerts = engine.evaluate(&snap, &history);
        let max_alerts = rows[2].height.saturating_sub(2) as usize;
        let alert_lines: Vec<Line> = if alerts.is_empty() {
            vec![Line::from("  ✓ No active alerts").style(Style::default().fg(Color::Green))]
        } else {
            alerts
                .iter()
                .take(max_alerts)
                .map(|a| {
                    let color = match a.severity {
                        drishti_core::anomaly::Severity::Critical => Color::Red,
                        drishti_core::anomaly::Severity::High => Color::LightRed,
                        drishti_core::anomaly::Severity::Warn => Color::Yellow,
                        drishti_core::anomaly::Severity::Info => Color::Cyan,
                    };
                    Line::from(format!("  [{}] {} — {}", a.severity, a.title, a.detail))
                        .style(Style::default().fg(color))
                })
                .collect()
        };
        frame.render_widget(Paragraph::new(alert_lines).block(alert_block), rows[2]);
    }
}

fn sev_color(pct: f64) -> Color {
    if pct > 85.0 {
        Color::Red
    } else if pct > 65.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}
