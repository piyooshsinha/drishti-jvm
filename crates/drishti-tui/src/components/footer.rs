use super::Component;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct Footer;
impl Component for Footer {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let keys = " Tab:Switch  1-7:Tab  j/k:Scroll  ?:Help  q:Quit ";
        frame.render_widget(
            Paragraph::new(keys).style(Style::default().fg(Color::DarkGray)).alignment(Alignment::Center),
            area,
        );
    }
}
