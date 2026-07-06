use super::Component;
use crate::collector::{AppState, SourceState};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::sync::Arc;

pub struct Header {
    pub state: Arc<AppState>,
}

/// Indicator glyph + color for one data source.
fn source_indicator(s: SourceState) -> (&'static str, Color) {
    match s {
        SourceState::Up => ("●", Color::Green),
        SourceState::Down => ("●", Color::Red),
        SourceState::Connecting => ("◌", Color::Yellow),
        SourceState::Disabled => ("○", Color::DarkGray),
    }
}

impl Component for Header {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let act = self.state.actuator_status.get();
        let jol = self.state.jolokia_status.get();

        let any_up = act == SourceState::Up || jol == SourceState::Up;
        let base_color = if any_up { Color::Green } else { Color::Red };

        let uptime = if snap.jvm_info.uptime_ms > 0 {
            format!(" ▲{}", snap.jvm_info.uptime_human())
        } else {
            String::new()
        };

        let gc_str = if snap.jvm_info.gc_algorithm != drishti_core::model::GcAlgorithm::Unknown {
            format!(" {}", snap.jvm_info.gc_algorithm_str())
        } else {
            String::new()
        };

        let readonly = if self.state.readonly {
            " [READONLY]"
        } else {
            ""
        };

        let vm_name = if !snap.jvm_info.vm_name.is_empty() {
            snap.jvm_info.vm_name.clone()
        } else {
            "connecting...".to_string()
        };

        let (act_dot, act_color) = source_indicator(act);
        let (jol_dot, jol_color) = source_indicator(jol);

        let line = Line::from(vec![
            Span::styled(" दृष्टि drishti-jvm ", Style::default().fg(base_color)),
            Span::styled("│ ACT ", Style::default().fg(Color::DarkGray)),
            Span::styled(act_dot, Style::default().fg(act_color)),
            Span::styled(" JOL ", Style::default().fg(Color::DarkGray)),
            Span::styled(jol_dot, Style::default().fg(jol_color)),
            Span::styled(
                format!(" │ {}{}{}{} ", vm_name, uptime, gc_str, readonly),
                Style::default().fg(base_color),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }
}
