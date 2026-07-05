//! Help overlay — pops up on `?` to show all keybindings.

use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw_help_overlay(frame: &mut Frame) {
    let area = frame.area();
    // Center a box that's 60% width, 70% height
    let w = (area.width as f32 * 0.6).min(60.0) as u16;
    let h = (area.height as f32 * 0.7).min(25.0) as u16;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup_area = Rect::new(x, y, w, h);

    // Dim background
    frame.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from("  दृष्टि drishti-jvm — Keybindings").style(Style::default().fg(Color::Cyan).bold()),
        Line::from(""),
        Line::from("  Navigation").style(Style::default().fg(Color::Yellow).bold()),
        Line::from("    Tab / Shift+Tab    Cycle tabs"),
        Line::from("    1-7                Jump to tab directly"),
        Line::from("    j / ↓              Scroll down / Select next"),
        Line::from("    k / ↑              Scroll up / Select prev"),
        Line::from("    g / Home           Scroll to top"),
        Line::from("    G / End            Scroll to bottom"),
        Line::from(""),
        Line::from("  Actions").style(Style::default().fg(Color::Yellow).bold()),
        Line::from("    ?                  Toggle this help"),
        Line::from("    q                  Quit"),
        Line::from("    Enter              Expand selection"),
        Line::from("    Esc                Close popup / back"),
        Line::from(""),
        Line::from("  Tabs").style(Style::default().fg(Color::Yellow).bold()),
        Line::from("    1:Overview  2:Memory  3:Threads"),
        Line::from("    4:HTTP  5:DB/Pool  6:Logs  7:Recommend"),
        Line::from(""),
        Line::from("  Press ? or Esc to close").style(Style::default().fg(Color::DarkGray)),
    ];

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(help_text).block(block);
    frame.render_widget(paragraph, popup_area);
}
