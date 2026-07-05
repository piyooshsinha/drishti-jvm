use crate::collector::AppState;
use drishti_core::model::PoolType;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct MemoryTab { pub state: Arc<AppState> }

impl MemoryTab {
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();
        let chunks = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(8)])
            .split(area);

        // Top: Heap + Non-heap summary gauges
        let top = Layout::default().direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        let heap_pct = snap.heap.usage_pct().unwrap_or(0.0);
        frame.render_widget(Gauge::default()
            .block(Block::default().title(" Heap ").borders(Borders::ALL))
            .gauge_style(Style::default().fg(severity_color(heap_pct)))
            .percent(heap_pct.min(100.0) as u16)
            .label(format!("{:.0}M / {:.0}M", snap.heap.used_mb(), snap.heap.max_mb())),
            top[0]);

        let nh_pct = snap.non_heap.usage_pct().unwrap_or(0.0);
        frame.render_widget(Gauge::default()
            .block(Block::default().title(" Non-Heap ").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Blue))
            .percent(nh_pct.min(100.0).max(0.0) as u16)
            .label(format!("{:.0}M / {:.0}M", snap.non_heap.used_mb(), snap.non_heap.max_mb())),
            top[1]);

        // Middle: Memory pool table
        let pool_header = Row::new(vec!["Pool", "Type", "Used", "Max", "Usage"])
            .style(Style::default().fg(Color::Cyan).bold());
        let pool_rows: Vec<Row> = snap.memory_pools.iter().map(|p| {
            let pct = p.usage.usage_pct().map(|v| format!("{:.0}%", v)).unwrap_or("-".into());
            let type_str = match p.pool_type { PoolType::Heap => "Heap", PoolType::NonHeap => "Non-Heap", _ => "?" };
            let color = p.usage.usage_pct().map(|v| severity_color(v)).unwrap_or(Color::White);
            Row::new(vec![
                p.name.clone(), type_str.to_string(),
                format!("{:.1}M", p.usage.used_mb()), format!("{:.1}M", p.usage.max_mb()), pct,
            ]).style(Style::default().fg(color))
        }).collect();

        let pool_table = Table::new(pool_rows, [
            Constraint::Percentage(30), Constraint::Percentage(15), Constraint::Percentage(15),
            Constraint::Percentage(15), Constraint::Percentage(15),
        ]).header(pool_header)
          .block(Block::default().title(" Memory Pools ").borders(Borders::ALL));
        frame.render_widget(pool_table, chunks[1]);

        // Bottom: GC collectors
        let gc_header = Row::new(vec!["Collector", "Count", "Total Time", "Avg Pause"])
            .style(Style::default().fg(Color::Cyan).bold());
        let gc_rows: Vec<Row> = snap.gc_collectors.iter().map(|c| {
            let avg = if c.collection_count > 0 { c.collection_time_ms as f64 / c.collection_count as f64 } else { 0.0 };
            Row::new(vec![
                c.name.clone(), c.collection_count.to_string(),
                format!("{:.0}ms", c.collection_time_ms), format!("{:.1}ms", avg),
            ])
        }).collect();
        let gc_table = Table::new(gc_rows, [
            Constraint::Percentage(30), Constraint::Percentage(20),
            Constraint::Percentage(25), Constraint::Percentage(25),
        ]).header(gc_header)
          .block(Block::default().title(" GC Collectors ").borders(Borders::ALL));
        frame.render_widget(gc_table, chunks[2]);
    }
}

fn severity_color(pct: f64) -> Color {
    if pct > 85.0 { Color::Red } else if pct > 65.0 { Color::Yellow } else { Color::Green }
}
