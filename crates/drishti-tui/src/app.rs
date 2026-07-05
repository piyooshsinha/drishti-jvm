use crate::action::Action;
use crate::collector::{AppState, CollectorChannels};
use crate::components::footer::Footer;
use crate::components::header::Header;
use crate::components::help::draw_help_overlay;
use crate::components::Component;
use crate::tabs::console::ConsoleTab;
use crate::tabs::db::DbTab;
use crate::tabs::http::HttpTab;
use crate::tabs::logs::LogsTab;
use crate::tabs::mbeans::MBeansTab;
use crate::tabs::memory::MemoryTab;
use crate::tabs::overview::OverviewTab;
use crate::tabs::profiler::ProfilerTab;
use crate::tabs::recommendations::RecommendationsTab;
use crate::tabs::threads::ThreadsTab;
use crate::tabs::Tab;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Tabs as TabsWidget;
use std::sync::Arc;

pub struct App {
    pub running: bool,
    pub active_tab: Tab,
    pub show_help: bool,
    state: Arc<AppState>,
    header: Header,
    footer: Footer,
    overview: OverviewTab,
    memory: MemoryTab,
    threads: ThreadsTab,
    http: HttpTab,
    db: DbTab,
    logs: LogsTab,
    mbeans: MBeansTab,
    profiler: ProfilerTab,
    console: ConsoleTab,
    recommendations: RecommendationsTab,
    console_input_mode: bool,
    channels: CollectorChannels,
}

impl App {
    pub fn new(state: Arc<AppState>, channels: CollectorChannels) -> Self {
        Self {
            running: true,
            active_tab: Tab::Overview,
            show_help: false,
            header: Header {
                state: state.clone(),
            },
            footer: Footer,
            overview: OverviewTab {
                state: state.clone(),
            },
            memory: MemoryTab {
                state: state.clone(),
            },
            threads: ThreadsTab {
                state: state.clone(),
                scroll_offset: 0,
            },
            http: HttpTab {
                state: state.clone(),
                scroll_offset: 0,
            },
            db: DbTab {
                state: state.clone(),
            },
            logs: LogsTab::new(state.clone()),
            mbeans: MBeansTab::new(state.clone()),
            profiler: ProfilerTab::new(state.clone()),
            console: ConsoleTab::new(state.clone()),
            recommendations: RecommendationsTab {
                state: state.clone(),
            },
            state,
            console_input_mode: false,
            channels,
        }
    }

    /// Drain data pushed by background collectors into UI-owned buffers.
    fn drain_channels(&mut self) {
        while let Ok(line) = self.channels.log_rx.try_recv() {
            self.logs.add_log(crate::tabs::logs::parse_log_line(&line));
        }
        if let Ok(names) = self.channels.mbeans_rx.try_recv() {
            self.mbeans.load_mbeans(names);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => {
                    self.show_help = false;
                }
                _ => {}
            }
            return Action::Render;
        }

        // Console input mode
        if self.active_tab == Tab::Console && self.console_input_mode {
            match key.code {
                KeyCode::Esc => {
                    self.console_input_mode = false;
                }
                KeyCode::Enter => {
                    self.console.execute();
                }
                KeyCode::Backspace => {
                    self.console.backspace();
                }
                KeyCode::Left => {
                    self.console.cursor_left();
                }
                KeyCode::Right => {
                    self.console.cursor_right();
                }
                KeyCode::Up => {
                    self.console.history_up();
                }
                KeyCode::Down => {
                    self.console.history_down();
                }
                KeyCode::Char(c) => {
                    self.console.insert_char(c);
                }
                _ => {}
            }
            return Action::Render;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => Action::Quit,
            KeyCode::Char('?') => {
                self.show_help = true;
                Action::Render
            }
            KeyCode::Tab => {
                self.active_tab = self.active_tab.next();
                Action::Render
            }
            KeyCode::BackTab => {
                self.active_tab = self.active_tab.prev();
                Action::Render
            }
            // Number keys for tab switching (0 = tab 10)
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as u8 - b'1') as usize;
                if idx < Tab::ALL.len() {
                    self.active_tab = Tab::from_index(idx);
                }
                Action::Render
            }
            KeyCode::Char('0') => {
                if Tab::ALL.len() >= 10 {
                    self.active_tab = Tab::from_index(9);
                }
                Action::Render
            }
            // Scroll
            KeyCode::Char('j') | KeyCode::Down => {
                match self.active_tab {
                    Tab::Threads => {
                        self.threads.scroll_offset += 1;
                    }
                    Tab::Http => {
                        self.http.scroll_offset += 1;
                    }
                    Tab::Logs => {
                        self.logs.scroll_down();
                    }
                    Tab::MBeans => {
                        self.mbeans.select_next();
                    }
                    _ => {}
                }
                Action::Render
            }
            KeyCode::Char('k') | KeyCode::Up => {
                match self.active_tab {
                    Tab::Threads => {
                        self.threads.scroll_offset = self.threads.scroll_offset.saturating_sub(1);
                    }
                    Tab::Http => {
                        self.http.scroll_offset = self.http.scroll_offset.saturating_sub(1);
                    }
                    Tab::Logs => {
                        self.logs.scroll_up();
                    }
                    Tab::MBeans => {
                        self.mbeans.select_prev();
                    }
                    _ => {}
                }
                Action::Render
            }
            KeyCode::Char('g') | KeyCode::Home => {
                match self.active_tab {
                    Tab::Threads => {
                        self.threads.scroll_offset = 0;
                    }
                    Tab::Http => {
                        self.http.scroll_offset = 0;
                    }
                    _ => {}
                }
                Action::Render
            }
            KeyCode::Char('G') | KeyCode::End => {
                if self.active_tab == Tab::Logs {
                    self.logs.scroll_bottom();
                }
                Action::Render
            }
            KeyCode::Char('L') if self.active_tab == Tab::Logs => {
                self.logs.cycle_min_level();
                Action::Render
            }
            KeyCode::Char('e') if self.active_tab == Tab::Profiler => {
                self.profiler.cycle_event();
                Action::Render
            }
            KeyCode::Char('+') | KeyCode::Char('=') if self.active_tab == Tab::Profiler => {
                self.profiler.increase_duration();
                Action::Render
            }
            KeyCode::Char('-') if self.active_tab == Tab::Profiler => {
                self.profiler.decrease_duration();
                Action::Render
            }
            KeyCode::Enter => {
                match self.active_tab {
                    Tab::MBeans => {
                        self.mbeans.toggle_selected();
                    }
                    Tab::Console => {
                        self.console_input_mode = true;
                    }
                    Tab::Profiler => {
                        self.profiler.start_recording();
                    }
                    _ => {}
                }
                Action::Render
            }
            KeyCode::Char('o') if self.active_tab == Tab::Profiler => {
                self.profiler.open_result();
                Action::Render
            }
            KeyCode::Char('i') if self.active_tab == Tab::Console => {
                self.console_input_mode = true;
                Action::Render
            }
            _ => Action::None,
        }
    }

    pub fn update(&mut self, action: &Action) {
        match action {
            Action::Quit => {
                self.running = false;
            }
            Action::Tick => {
                self.drain_channels();
            }
            _ => {}
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(frame.area());

        self.header.draw(frame, chunks[0]);

        // Tab bar with number shortcuts
        let tab_titles: Vec<Line> = Tab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let style = if *t == self.active_tab {
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let num = if i < 9 {
                    format!("{}", i + 1)
                } else {
                    "0".to_string()
                };
                Line::from(format!("{}:{}", num, t.title())).style(style)
            })
            .collect();
        frame.render_widget(
            TabsWidget::new(tab_titles)
                .select(self.active_tab.index())
                .divider("│")
                .highlight_style(Style::default().fg(Color::Cyan).bold()),
            chunks[1],
        );

        match self.active_tab {
            Tab::Overview => self.overview.draw(frame, chunks[2]),
            Tab::Memory => self.memory.draw(frame, chunks[2]),
            Tab::Threads => self.threads.draw(frame, chunks[2]),
            Tab::Http => self.http.draw(frame, chunks[2]),
            Tab::Db => self.db.draw(frame, chunks[2]),
            Tab::Logs => self.logs.draw(frame, chunks[2]),
            Tab::MBeans => self.mbeans.draw(frame, chunks[2]),
            Tab::Profiler => self.profiler.draw(frame, chunks[2]),
            Tab::Console => self.console.draw(frame, chunks[2]),
            Tab::Recommendations => self.recommendations.draw(frame, chunks[2]),
        }

        self.footer.draw(frame, chunks[3]);
        if self.show_help {
            draw_help_overlay(frame);
        }
    }
}
