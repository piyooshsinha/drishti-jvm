use crate::collector::AppState;
use drishti_core::anomaly::Severity;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct RecommendationsTab { pub state: Arc<AppState> }

impl RecommendationsTab {
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let history = self.state.get_history();

        let chunks = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);

        // Anomaly alerts
        let anomaly_engine = drishti_core::detectors::default_engine();
        let alerts = anomaly_engine.evaluate(&snap, &history);

        let alert_header = Row::new(vec!["Sev", "Alert", "Detail", "Conf"])
            .style(Style::default().fg(Color::Cyan).bold());
        let alert_rows: Vec<Row> = alerts.iter().map(|a| {
            let color = sev_color(a.severity);
            Row::new(vec![
                format!("{}", a.severity),
                a.title.clone(),
                a.detail.chars().take(50).collect::<String>(),
                format!("{:.0}%", a.confidence * 100.0),
            ]).style(Style::default().fg(color))
        }).collect();

        let alert_title = format!(" Alerts ({}) ", alerts.len());
        let alert_table = Table::new(alert_rows, [
            Constraint::Length(5), Constraint::Percentage(30),
            Constraint::Percentage(50), Constraint::Length(6),
        ]).header(alert_header)
          .block(Block::default().title(alert_title).borders(Borders::ALL));
        frame.render_widget(alert_table, chunks[0]);

        // Tuning recommendations
        let rec_engine = drishti_core::rules::default_engine();
        let recs = rec_engine.evaluate(&snap, &history);

        let rec_header = Row::new(vec!["Category", "Recommendation", "Flags", "Conf"])
            .style(Style::default().fg(Color::Cyan).bold());
        let rec_rows: Vec<Row> = recs.iter().map(|r| {
            let color = sev_color(r.severity);
            let flags = r.jvm_flags.join(" ");
            Row::new(vec![
                format!("{:?}", r.category),
                r.title.clone(),
                if flags.is_empty() { r.suggestion.chars().take(30).collect() } else { flags },
                format!("{:.0}%", r.confidence * 100.0),
            ]).style(Style::default().fg(color))
        }).collect();

        let rec_title = format!(" Tuning Recommendations ({}) ", recs.len());
        let rec_table = Table::new(rec_rows, [
            Constraint::Length(14), Constraint::Percentage(35),
            Constraint::Percentage(35), Constraint::Length(6),
        ]).header(rec_header)
          .block(Block::default().title(rec_title).borders(Borders::ALL));
        frame.render_widget(rec_table, chunks[1]);
    }
}

fn sev_color(s: Severity) -> Color {
    match s {
        Severity::Critical => Color::Red,
        Severity::High => Color::LightRed,
        Severity::Warn => Color::Yellow,
        Severity::Info => Color::Cyan,
    }
}
