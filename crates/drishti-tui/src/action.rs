//! Action enum — the central message bus for the TUI.
//!
//! Every user interaction, data update, or system event becomes an Action
//! that flows through the App's component dispatch loop.

/// Actions that drive the TUI state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ── Lifecycle ─────────────────────────────────────────
    Tick,
    Render,
    Quit,

    // ── Navigation ────────────────────────────────────────
    TabNext,
    TabPrev,
    TabSelect(usize),

    // ── Data ──────────────────────────────────────────────
    DataRefreshed,
    ConnectionLost(String),
    ConnectionRestored,

    // ── UI ────────────────────────────────────────────────
    ToggleHelp,
    ScrollUp,
    ScrollDown,
    ScrollTop,
    ScrollBottom,
    SelectNext,
    SelectPrev,
    Enter,
    Back,
    Filter(String),
    ClearFilter,

    // ── Theme ─────────────────────────────────────────────
    SwitchTheme(String),

    // ── Alerts ────────────────────────────────────────────
    NewAlert(String),
    DismissAlert,

    // ── Resize ────────────────────────────────────────────
    Resize(u16, u16),

    // ── No-op ─────────────────────────────────────────────
    None,
}
