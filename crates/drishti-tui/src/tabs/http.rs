use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct HttpTab {
    pub state: Arc<AppState>,
    pub scroll_offset: usize,
}

impl HttpTab {
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let derived = self.state.current_derived();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(5)])
            .split(area);

        // Summary with derived rates
        let summary_block = Block::default()
            .title(" HTTP Summary ")
            .borders(Borders::ALL);
        let error_style = if snap.http.total_errors > 0 {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };
        let summary = vec![
            Line::from(format!(
                "  Total Requests: {}    Errors: {}    Error Rate: {:.2}%",
                snap.http.total_requests,
                snap.http.total_errors,
                snap.http.error_rate * 100.0
            ))
            .style(error_style),
            Line::from(format!(
                "  Request Rate: {:.1} req/s    Avg Latency: {:.1}ms    Endpoints: {}",
                derived.http_requests_per_sec,
                snap.http.avg_latency_ms,
                snap.http.endpoints.len()
            )),
        ];
        frame.render_widget(Paragraph::new(summary).block(summary_block), chunks[0]);

        // Scrollable endpoint table
        let header = Row::new(vec![
            "URI", "Method", "Count", "Avg(ms)", "Max(ms)", "Errors",
        ])
        .style(Style::default().fg(Color::Cyan).bold());

        let visible_height = chunks[1].height.saturating_sub(3) as usize;
        let total = snap.http.endpoints.len();
        let offset = self.scroll_offset.min(total.saturating_sub(visible_height));

        let rows: Vec<Row> = snap
            .http
            .endpoints
            .iter()
            .skip(offset)
            .take(visible_height)
            .map(|e| {
                let avg = if e.count > 0 {
                    e.total_time_ms / e.count as f64
                } else {
                    0.0
                };
                let color = if e.error_count > 0 {
                    Color::Red
                } else if avg > 500.0 {
                    Color::Yellow
                } else {
                    Color::White
                };
                Row::new(vec![
                    e.uri.clone(),
                    e.method.clone(),
                    e.count.to_string(),
                    format!("{:.1}", avg),
                    format!("{:.1}", e.max_ms),
                    e.error_count.to_string(),
                ])
                .style(Style::default().fg(color))
            })
            .collect();

        let scroll_info = if total > 0 {
            format!(
                " Endpoints {}-{} of {} (j/k to scroll) ",
                offset + 1,
                (offset + visible_height).min(total),
                total
            )
        } else {
            " Endpoints (waiting for data...) ".to_string()
        };

        let table = Table::new(
            rows,
            [
                Constraint::Min(25),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(8),
            ],
        )
        .header(header)
        .block(Block::default().title(scroll_info).borders(Borders::ALL));
        frame.render_widget(table, chunks[1]);
    }
}
