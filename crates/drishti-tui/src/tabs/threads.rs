use crate::collector::AppState;
use drishti_core::model::ThreadState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::sync::Arc;

pub struct ThreadsTab {
    pub state: Arc<AppState>,
    pub scroll_offset: usize,
}

impl ThreadsTab {
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let snap = self.state.current();

        let has_deadlock = !snap.deadlocks.is_empty();
        let constraints = if has_deadlock {
            vec![Constraint::Length(3), Constraint::Length(5), Constraint::Min(3)]
        } else {
            vec![Constraint::Length(5), Constraint::Min(3)]
        };
        let chunks = Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);
        let mut chunk_idx = 0;

        // Deadlock banner
        if has_deadlock {
            let total: usize = snap.deadlocks.iter().map(|d| d.thread_ids.len()).sum();
            let banner = Paragraph::new(format!(" ⚠ DEADLOCK DETECTED: {} threads involved", total))
                .style(Style::default().fg(Color::White).bg(Color::Red).bold())
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(banner, chunks[chunk_idx]);
            chunk_idx += 1;
        }

        // Thread state bar chart
        let states = vec![
            ("RUN", ThreadState::Runnable, Color::Green),
            ("WAIT", ThreadState::Waiting, Color::Yellow),
            ("TIMED_W", ThreadState::TimedWaiting, Color::Blue),
            ("BLOCK", ThreadState::Blocked, Color::Red),
            ("NEW", ThreadState::New, Color::Cyan),
        ];
        let bars: Vec<Bar> = states.iter().map(|(label, ts, color)| {
            let val = *snap.thread_summary.state_counts.get(ts).unwrap_or(&0) as u64;
            Bar::default().value(val).label(Line::from(*label)).style(Style::default().fg(*color))
        }).collect();

        let bar_group = BarGroup::default().bars(&bars);
        let bar_chart = BarChart::default()
            .block(Block::default()
                .title(format!(" Thread States (Live:{} Daemon:{} Peak:{}) ",
                    snap.thread_summary.live, snap.thread_summary.daemon, snap.thread_summary.peak))
                .borders(Borders::ALL))
            .bar_width(8).bar_gap(1)
            .data(bar_group);
        frame.render_widget(bar_chart, chunks[chunk_idx]);
        chunk_idx += 1;

        // Scrollable thread list
        let header = Row::new(vec!["ID", "Name", "State", "Daemon", "Blocked", "Waited"])
            .style(Style::default().fg(Color::Cyan).bold())
            .bottom_margin(0);

        let visible_height = chunks[chunk_idx].height.saturating_sub(3) as usize;
        let total_threads = snap.threads.len();
        let offset = self.scroll_offset.min(total_threads.saturating_sub(visible_height));

        let rows: Vec<Row> = snap.threads.iter()
            .skip(offset)
            .take(visible_height)
            .map(|t| {
                let state_color = match t.state {
                    ThreadState::Runnable => Color::Green,
                    ThreadState::Blocked => Color::Red,
                    ThreadState::Waiting => Color::Yellow,
                    ThreadState::TimedWaiting => Color::Blue,
                    _ => Color::White,
                };
                Row::new(vec![
                    t.id.to_string(),
                    t.name.chars().take(40).collect::<String>(),
                    format!("{:?}", t.state),
                    if t.daemon { "Y" } else { "N" }.to_string(),
                    t.blocked_count.to_string(),
                    t.waited_count.to_string(),
                ]).style(Style::default().fg(state_color))
            }).collect();

        let scroll_info = if total_threads > 0 {
            format!(" Threads {}-{} of {} (j/k to scroll) ", offset + 1,
                (offset + visible_height).min(total_threads), total_threads)
        } else {
            " Threads (waiting for thread dump...) ".to_string()
        };

        let table = Table::new(rows, [
            Constraint::Length(8), Constraint::Min(20), Constraint::Length(14),
            Constraint::Length(7), Constraint::Length(10), Constraint::Length(10),
        ]).header(header).block(Block::default().title(scroll_info).borders(Borders::ALL));
        frame.render_widget(table, chunks[chunk_idx]);
    }
}
