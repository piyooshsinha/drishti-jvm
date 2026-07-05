pub mod overview;
pub mod memory;
pub mod threads;
pub mod http;
pub mod db;
pub mod logs;
pub mod mbeans;
pub mod profiler;
pub mod console;
pub mod recommendations;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview, Memory, Threads, Http, Db, Logs, MBeans, Profiler, Console, Recommendations,
}

impl Tab {
    pub const ALL: &[Tab] = &[
        Tab::Overview, Tab::Memory, Tab::Threads, Tab::Http, Tab::Db,
        Tab::Logs, Tab::MBeans, Tab::Profiler, Tab::Console, Tab::Recommendations,
    ];
    pub fn title(&self) -> &str {
        match self {
            Tab::Overview => "Overview", Tab::Memory => "Memory", Tab::Threads => "Threads",
            Tab::Http => "HTTP", Tab::Db => "DB/Pool", Tab::Logs => "Logs",
            Tab::MBeans => "MBeans", Tab::Profiler => "Profiler", Tab::Console => "Console",
            Tab::Recommendations => "Recommend",
        }
    }
    pub fn index(&self) -> usize { Tab::ALL.iter().position(|t| t == self).unwrap_or(0) }
    pub fn from_index(i: usize) -> Self { Tab::ALL.get(i).copied().unwrap_or(Tab::Overview) }
    pub fn next(&self) -> Self { Tab::from_index((self.index() + 1) % Tab::ALL.len()) }
    pub fn prev(&self) -> Self { let i = self.index(); Tab::from_index(if i == 0 { Tab::ALL.len() - 1 } else { i - 1 }) }
}
