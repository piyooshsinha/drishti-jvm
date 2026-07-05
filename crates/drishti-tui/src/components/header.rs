use super::Component;
use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::sync::Arc;

pub struct Header {
    pub state: Arc<AppState>,
}

impl Component for Header {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let connected = snap.jvm_info.uptime_ms > 0 || snap.heap.used > 0;
        let status = if connected { "●" } else { "○" };
        let color = if connected { Color::Green } else { Color::Red };

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

        let text = format!(
            " दृष्टि drishti-jvm │ {} {}{}{}{} ",
            status,
            if !snap.jvm_info.vm_name.is_empty() {
                &snap.jvm_info.vm_name
            } else {
                "connecting..."
            },
            uptime,
            gc_str,
            readonly,
        );
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(color)), area);
    }
}
