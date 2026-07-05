//! MBeans browser tab — navigate JMX MBeans, read attributes, invoke operations.
//!
//! Left pane: domain → MBean tree (collapsed/expanded)
//! Right pane: selected MBean's attributes and values

use crate::collector::AppState;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A node in the MBean tree.
#[derive(Debug, Clone)]
pub struct MBeanNode {
    pub name: String,
    pub full_path: String,
    pub is_domain: bool,
    pub expanded: bool,
    pub children: Vec<MBeanNode>,
    pub attributes: BTreeMap<String, String>, // attribute name → value as string
}

pub struct MBeansTab {
    pub state: Arc<AppState>,
    pub tree: Vec<MBeanNode>,
    pub selected_index: usize,
    pub tree_loaded: bool,
    /// Flattened view of visible nodes for rendering/navigation.
    flat_view: Vec<FlatNode>,
    /// Currently selected MBean's attributes.
    pub selected_attrs: BTreeMap<String, String>,
    pub attr_scroll: usize,
}

#[derive(Debug, Clone)]
struct FlatNode {
    name: String,
    full_path: String,
    depth: usize,
    is_domain: bool,
    expanded: bool,
    has_children: bool,
}

impl MBeansTab {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tree: Vec::new(),
            selected_index: 0,
            tree_loaded: false,
            flat_view: Vec::new(),
            selected_attrs: BTreeMap::new(),
            attr_scroll: 0,
        }
    }

    /// Build the tree from a list of MBean names (from Jolokia search).
    pub fn load_mbeans(&mut self, mbean_names: Vec<String>) {
        let mut domains: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for name in &mbean_names {
            let (domain, rest) = name.split_once(':').unwrap_or((name, ""));
            domains.entry(domain.to_string()).or_default().push(rest.to_string());
        }

        self.tree = domains.into_iter().map(|(domain, beans)| {
            let children: Vec<MBeanNode> = beans.into_iter().map(|bean| {
                let short_name = bean.split(',')
                    .find(|s| s.starts_with("type=") || s.starts_with("name="))
                    .unwrap_or(&bean)
                    .to_string();
                MBeanNode {
                    name: short_name,
                    full_path: format!("{}:{}", domain, bean),
                    is_domain: false,
                    expanded: false,
                    children: vec![],
                    attributes: BTreeMap::new(),
                }
            }).collect();

            MBeanNode {
                name: domain.clone(),
                full_path: domain,
                is_domain: true,
                expanded: false,
                children,
                attributes: BTreeMap::new(),
            }
        }).collect();

        self.tree_loaded = true;
        self.rebuild_flat_view();
    }

    fn rebuild_flat_view(&mut self) {
        let mut flat = Vec::new();
        for node in &self.tree {
            Self::flatten_node(&mut flat, node, 0);
        }
        self.flat_view = flat;
    }

    fn flatten_node(flat: &mut Vec<FlatNode>, node: &MBeanNode, depth: usize) {
        flat.push(FlatNode {
            name: node.name.clone(),
            full_path: node.full_path.clone(),
            depth,
            is_domain: node.is_domain,
            expanded: node.expanded,
            has_children: !node.children.is_empty(),
        });
        if node.expanded {
            for child in &node.children {
                Self::flatten_node(flat, child, depth + 1);
            }
        }
    }

    pub fn toggle_selected(&mut self) {
        if self.selected_index >= self.flat_view.len() { return; }
        let path = self.flat_view[self.selected_index].full_path.clone();
        let is_domain = self.flat_view[self.selected_index].is_domain;

        if is_domain {
            // Toggle domain expansion
            if let Some(domain) = self.tree.iter_mut().find(|d| d.full_path == path) {
                domain.expanded = !domain.expanded;
            }
        }
        self.rebuild_flat_view();
    }

    pub fn select_next(&mut self) {
        if self.selected_index < self.flat_view.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn set_attributes(&mut self, attrs: BTreeMap<String, String>) {
        self.selected_attrs = attrs;
        self.attr_scroll = 0;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        if !self.tree_loaded {
            frame.render_widget(
                Paragraph::new("  Loading MBeans... (requires Jolokia connection)")
                    .block(Block::default().title(" MBeans ").borders(Borders::ALL)),
                area,
            );
            return;
        }

        // Split: left tree (40%) | right attributes (60%)
        let chunks = Layout::default().direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        // Left: MBean tree
        let visible_height = chunks[0].height.saturating_sub(2) as usize;
        let scroll = self.selected_index.saturating_sub(visible_height / 2);

        let items: Vec<ListItem> = self.flat_view.iter().enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, node)| {
                let indent = "  ".repeat(node.depth);
                let icon = if node.is_domain {
                    if node.expanded { "▼ " } else { "▶ " }
                } else if node.has_children {
                    if node.expanded { "▼ " } else { "▶ " }
                } else {
                    "  "
                };
                let style = if i == self.selected_index {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else if node.is_domain {
                    Style::default().fg(Color::Yellow).bold()
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("{}{}{}", indent, icon, node.name)).style(style)
            }).collect();

        let tree_block = Block::default()
            .title(format!(" MBeans ({} domains) ", self.tree.len()))
            .borders(Borders::ALL);
        frame.render_widget(List::new(items).block(tree_block), chunks[0]);

        // Right: selected MBean attributes
        let attr_block = Block::default()
            .title(if self.selected_index < self.flat_view.len() {
                format!(" {} ", self.flat_view[self.selected_index].name)
            } else {
                " Attributes ".to_string()
            })
            .borders(Borders::ALL);

        if self.selected_attrs.is_empty() {
            frame.render_widget(
                Paragraph::new("  Select an MBean and press Enter to load attributes")
                    .block(attr_block),
                chunks[1],
            );
        } else {
            let attr_height = chunks[1].height.saturating_sub(2) as usize;
            let rows: Vec<Row> = self.selected_attrs.iter()
                .skip(self.attr_scroll)
                .take(attr_height)
                .map(|(k, v)| {
                    let truncated_val: String = v.chars().take(60).collect();
                    Row::new(vec![k.clone(), truncated_val])
                }).collect();

            let table = Table::new(rows, [Constraint::Percentage(35), Constraint::Percentage(65)])
                .header(Row::new(vec!["Attribute", "Value"])
                    .style(Style::default().fg(Color::Cyan).bold()))
                .block(attr_block);
            frame.render_widget(table, chunks[1]);
        }
    }
}
