use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct DbTab {
    pub state: Arc<AppState>,
}

impl DbTab {
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Length(5),
                Constraint::Min(3),
            ])
            .split(area);

        // HikariCP
        if let Some(ref h) = snap.hikari {
            let util = h.utilization_pct();
            let color = if h.is_saturated() {
                Color::Red
            } else if util > 75.0 {
                Color::Yellow
            } else {
                Color::Green
            };

            let pool_block = Block::default()
                .title(format!(" HikariCP: {} ", h.pool_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));

            let gauges = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .margin(1)
                .split(chunks[0]);

            frame.render_widget(pool_block, chunks[0]);

            // Connection utilization gauge
            frame.render_widget(
                Gauge::default()
                    .block(Block::default().title("Connections"))
                    .gauge_style(Style::default().fg(color))
                    .percent(util.min(100.0) as u16)
                    .label(format!(
                        "Active:{} Idle:{} / Max:{}",
                        h.active, h.idle, h.max
                    )),
                gauges[0],
            );

            // Stats
            let stats = vec![
                Line::from(format!(
                    "Pending: {}  Timeouts: {}",
                    h.pending, h.timeout_count
                ))
                .style(if h.pending > 0 {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                }),
                Line::from(format!(
                    "Acquire: {:.1}ms  Usage: {:.1}ms  Create: {:.1}ms",
                    h.acquire_ms, h.usage_ms, h.creation_ms
                )),
            ];
            frame.render_widget(Paragraph::new(stats), gauges[1]);
        } else {
            frame.render_widget(
                Paragraph::new("  No HikariCP metrics available")
                    .block(Block::default().title(" HikariCP ").borders(Borders::ALL)),
                chunks[0],
            );
        }

        // Tomcat threads
        if let Some(ref t) = snap.tomcat {
            let util = if t.threads_max > 0 {
                t.threads_busy as f64 / t.threads_max as f64 * 100.0
            } else {
                0.0
            };
            let color = if util > 85.0 {
                Color::Red
            } else if util > 65.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            frame.render_widget(
                Gauge::default()
                    .block(
                        Block::default()
                            .title(" Tomcat Threads ")
                            .borders(Borders::ALL),
                    )
                    .gauge_style(Style::default().fg(color))
                    .percent(util.min(100.0) as u16)
                    .label(format!(
                        "Busy: {} / {} ({:.0}%)",
                        t.threads_busy, t.threads_max, util
                    )),
                chunks[1],
            );
        } else {
            frame.render_widget(
                Paragraph::new("  No Tomcat metrics available")
                    .block(Block::default().title(" Tomcat ").borders(Borders::ALL)),
                chunks[1],
            );
        }

        // Class loading
        let class_block = Block::default()
            .title(" Class Loading ")
            .borders(Borders::ALL);
        let class_text = vec![Line::from(format!(
            "  Loaded: {}  Total Loaded: {}  Unloaded: {}",
            snap.classes.loaded, snap.classes.total_loaded, snap.classes.unloaded
        ))];
        frame.render_widget(Paragraph::new(class_text).block(class_block), chunks[2]);
    }
}
