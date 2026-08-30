//! History panel.

use gpui::*;
#[allow(unused_imports)]
use gpui::prelude::*;

use crate::messages::HistoryEvent;
use crate::theme::color;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub name: String,
    pub description: String,
}

pub struct HistoryPanel {
    entries: Vec<HistoryEntry>,
    current_index: Option<usize>,
    focus_handle: FocusHandle,
}

impl HistoryPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self { entries: Vec::new(), current_index: None, focus_handle: cx.focus_handle() }
    }

    pub fn add_entry(&mut self, name: String, description: String, cx: &mut Context<Self>) {
        if let Some(idx) = self.current_index { self.entries.truncate(idx + 1); } else { self.entries.clear(); }
        self.entries.push(HistoryEntry { name: name.clone(), description: description.clone() });
        self.current_index = Some(self.entries.len() - 1);
        if self.entries.len() > 50 { self.entries.remove(0); self.current_index = Some(self.entries.len() - 1); }
        cx.emit(HistoryEvent::EntryAdded { name, description });
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(idx) = self.current_index {
            if idx > 0 {
                self.current_index = Some(idx - 1);
                cx.emit(HistoryEvent::Undone { name: self.entries[idx].name.clone() });
            }
        }
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if let Some(idx) = self.current_index {
            if idx < self.entries.len() - 1 {
                self.current_index = Some(idx + 1);
                cx.emit(HistoryEvent::Redone { name: self.entries[idx + 1].name.clone() });
            }
        }
    }
}

impl EventEmitter<HistoryEvent> for HistoryPanel {}

impl Focusable for HistoryPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HistoryPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(color::BG_PANEL))
            .flex()
            .flex_col()
            .child(
                div().p_3().flex().items_center().justify_between()
                    .child(div().text_color(rgb(color::TEXT_PRIMARY)).text_sm().child("历史"))
                    .child(div().flex().gap_2()
                        .child(div().id("undo").px_2().py_1().rounded(px(4.0)).bg(rgb(color::BG_TOOLBAR))
                            .text_color(rgb(color::TEXT_SECONDARY)).text_xs()
                            .on_click(cx.listener(|this, _, _window, cx| { this.undo(cx); }))
                            .child("↩ 撤销"))
                        .child(div().id("redo").px_2().py_1().rounded(px(4.0)).bg(rgb(color::BG_TOOLBAR))
                            .text_color(rgb(color::TEXT_SECONDARY)).text_xs()
                            .on_click(cx.listener(|this, _, _window, cx| { this.redo(cx); }))
                            .child("↪ 重做"))),
            )
            .h(px(1.0)).bg(rgb(color::BORDER))
            .child(div().flex_1().p_2().flex().flex_col().gap_1()
                .children(self.entries.iter().enumerate().map(|(i, entry)| {
                    let is_current = self.current_index == Some(i);
                    div().p_2().rounded(px(4.0))
                        .bg(if is_current { rgb(color::ACCENT) } else { rgb(color::BG_TOOLBAR) })
                        .text_color(rgb(color::TEXT_PRIMARY)).text_xs()
                        .child(format!("{} {}", entry.name, entry.description))
                })))
    }
}
