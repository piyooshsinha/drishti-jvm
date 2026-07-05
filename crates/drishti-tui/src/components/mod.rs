pub mod footer;
pub mod header;
pub mod help;

use crate::action::Action;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

pub trait Component {
    fn handle_key_event(&mut self, _key: KeyEvent) -> Action {
        Action::None
    }
    fn update(&mut self, _action: &Action) -> Action {
        Action::None
    }
    fn draw(&self, frame: &mut Frame, area: Rect);
}
