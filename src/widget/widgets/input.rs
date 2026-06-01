//! Single or multi-line text editing widget.

use crate::prelude::*;
use std::any::Any;

/// Function pointer that constructs a boxed [`InputBindings`].
pub type InputBindingsFactory = fn() -> Box<dyn InputBindings<Text>>;

/// Process-wide visual and behavioral settings for [`Input`].
#[derive(Clone, Copy)]
pub struct InputConfig {
    /// Style applied to the selected text range.
    pub highlight_style: Style,
    /// Factory used to construct the bindings for new [`Input`] instances.
    pub bindings: InputBindingsFactory,
}

crate::config_module!(InputConfig {
    highlight_style: Style::new().fg(Color::BLUE).reverse(),
    bindings: DefaultBindings::new,
});

/// Single or multi-line text editing widget.
pub struct Input {
    text: Box<Text>,
    editor: Editor<Text>,
    multiline: bool,
    placeholder: Option<Box<Text>>,
    selected_style: Option<Style>,
}

impl Input {
    fn show_placeholder(&self) -> bool {
        self.text.len() == 0 && self.placeholder.is_some()
    }

    fn strip_newlines(&mut self) {
        let original = self.text.get_string();
        if !original.contains('\n') {
            return;
        }
        let stripped = original.replace('\n', "");
        let stripped_len = stripped.len();
        let cursor_idx = self.editor.get_cursor().get_index();
        let anchor_idx = self.editor.get_anchor().get_index();
        let newlines_before = |end: usize| -> usize {
            original[..end].bytes().filter(|&b| b == b'\n').count()
        };
        let new_cursor = (cursor_idx - newlines_before(cursor_idx)).min(stripped_len);
        let new_anchor = (anchor_idx - newlines_before(anchor_idx)).min(stripped_len);
        self.editor
            .replace_all_with_selection(&mut *self.text, &stripped, new_cursor, new_anchor);
    }

    fn update_highlight(&mut self) {
        self.text.clear_highlight();
        let range = self.editor.get_selected_range(&*self.text);
        if !range.is_empty() {
            let style = self.selected_style.unwrap_or_else(|| config::get().highlight_style);
            self.text.highlight(range, style);
        }
    }

    fn sync_multiline_overflow(&mut self) {
        if self.multiline {
            self.text.set_overflow(TextOverflow::WRAP);
        } else {
            self.text.set_overflow(TextOverflow::VISIBLE);
        }
        self.dirty_layout();
    }
}

impl DelegateWidget for Input {
    fn get_delegate(&self) -> &dyn Widget {
        if self.show_placeholder() {
            return &**self.placeholder.as_ref().unwrap();
        }
        &*self.text
    }

    fn get_delegate_mut(&mut self) -> &mut dyn Widget {
        if self.show_placeholder() {
            return &mut **self.placeholder.as_mut().unwrap();
        }
        &mut *self.text
    }

    fn override_is_focusable(&self) -> bool {
        true
    }

    fn after_on_state_change(&mut self, widget_state: WidgetState) {
        match widget_state {
            WidgetState::Focused
            | WidgetState::Active
            | WidgetState::FocusedHover => {
                self.editor.on_focus(&*self.text);
            }
            _ => {
                self.editor.on_blur(&*self.text);
                self.editor.collapse_selection();
            }
        }
        self.update_highlight();
        self.dirty_paint();
    }

    fn override_on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else { return InputResult::Rejected; };
        let is_hover = event.chord.trigger == Trigger::MouseHover;
        let is_release = matches!(event.chord.trigger, Trigger::MouseUp(_));
        let is_mouse = event.is_mouse_event();
        self.editor.take_dirty();
        let result = self.editor.on_input(&mut *self.text, queue);
        if result == InputResult::Handled {
            self.update_highlight();
            if self.editor.take_dirty() {
                self.dirty_layout();
                tuie::emit(self.get_id(), ChangeEvent(self.text.get_string()));
            }
        }
        if !is_hover && !is_release {
            tuie::focus_widget(self.get_id());
            tuie::reveal(self.get_id(), Vec2 { x: None, y: None });
        }
        if is_mouse {
            InputResult::Handled
        } else {
            result
        }
    }

    fn override_get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        if selected.is_some() {
            return None;
        }
        let cursor_idx = self.editor.get_cursor_index(&*self.text);
        let pos = self.text.index_to_virtual_pos(cursor_idx, self.editor.get_wrap_bias());
        Some((self.editor.get_cursor_shape(), pos.map(|v| v as i32)))
    }

    fn override_reveal(
        &mut self,
        _child: Option<WidgetId>,
        revelation: &mut Revelation,
        _scroll_align: Vec2<Option<Align>>,
    ) {
        let cursor_idx = self.editor.get_cursor_index(&*self.text);
        let cursor_pos = self
            .text
            .index_to_virtual_pos(cursor_idx, self.editor.get_wrap_bias())
            .map(|v| v as i32);
        revelation.push(Rect::new(cursor_pos, Vec2::of(1)));
    }

    fn override_measure_constraints(&mut self) -> Constraints {
        if !self.multiline {
            self.strip_newlines();
        }
        self.update_highlight();
        let child: &mut dyn Widget = if self.show_placeholder() {
            self.placeholder.as_mut().unwrap().as_mut()
        } else {
            &mut *self.text
        };
        let mut c = child.measure_constraints();
        c.min_size = c.min_size.map(|n: u16| n.max(1));
        c
    }
}

impl Input {
    /// Creates an empty single-line input using the configured default bindings.
    pub fn new() -> Box<Self> {
        let text = Text::new().overflow(TextOverflow::VISIBLE).margin_right(1);
        let editor = Editor::new(&*text, (config::get().bindings)());
        Box::new(Input {
            text,
            editor,
            multiline: false,
            placeholder: None,
            selected_style: None,
        })
    }

    /// Builder form of [`Input::set_content`].
    pub fn content(mut self: Box<Self>, content: impl Into<String>) -> Box<Self> {
        self.set_content(content.into());
        self
    }

    /// Replaces all contents with `content` and moves the cursor to the end.
    pub fn set_content(&mut self, content: impl Into<String>) {
        let content = content.into();
        self.editor.set_content(&mut *self.text, &content);
        self.text.clear_highlight();
        self.dirty_layout();
    }

    /// Selects the entire contents.
    pub fn select_all(&mut self) {
        self.editor.select_all(&*self.text);
        self.dirty_layout();
    }

    /// Returns the current contents as a [`String`].
    pub fn get_string(&self) -> String {
        self.text.get_string()
    }

    /// Returns the current contents as a borrowed `&str`.
    pub fn get_str(&self) -> &str {
        self.text.get_str()
    }

    crate::field! {
        /// Whether Enter inserts a newline.
        multiline: bool; sync_multiline_overflow
    }

    crate::delegate_field! {
        /// How text exceeding the input's width is handled.
        overflow: &'static TextOverflow => text
    }

    /// Builder shortcut for `.overflow(TextOverflow::WORD_WRAP)`.
    pub fn word_wrap(self: Box<Self>) -> Box<Self> {
        self.overflow(TextOverflow::WORD_WRAP)
    }

    /// Builder shortcut for `.overflow(TextOverflow::WRAP)`.
    pub fn wrap(self: Box<Self>) -> Box<Self> {
        self.overflow(TextOverflow::WRAP)
    }

    crate::delegate_field! {
        /// Horizontal alignment of the text.
        align: Align => text
    }

    /// Sets the placeholder shown when the input is empty.
    pub fn placeholder(mut self: Box<Self>, placeholder: Box<Text>) -> Box<Self> {
        self.set_placeholder(Some(placeholder));
        self
    }

    /// Sets the [`Text`] shown when the input is empty.
    pub fn set_placeholder(&mut self, placeholder: Option<Box<Text>>) {
        self.placeholder = placeholder;
        self.dirty_layout();
    }

    crate::style_field! {
        /// Style for selected text, or `None` for the default.
        selected_style: Option<Style>
    }

    /// Builder form of [`Input::set_bindings`].
    pub fn bindings(mut self: Box<Self>, factory: InputBindingsFactory) -> Box<Self> {
        self.set_bindings(factory);
        self
    }

    /// Replaces the bindings using `factory`.
    pub fn set_bindings(&mut self, factory: InputBindingsFactory) {
        self.editor.set_bindings(factory());
    }

    /// Returns the underlying [`Editor`].
    pub fn get_editor(&self) -> &Editor<Text> {
        &self.editor
    }

    /// Returns the editor paired with the text document for mutation.
    pub fn get_editor_mut(&mut self) -> (&mut Editor<Text>, &mut Text) {
        self.dirty_layout();
        (&mut self.editor, &mut *self.text)
    }

    /// Returns the current bindings paired with the text document.
    pub fn get_bindings(&self) -> (&dyn InputBindings<Text>, &Text) {
        (self.editor.get_bindings(), &*self.text)
    }

    /// Returns the current bindings paired with the text document for mutation.
    pub fn get_bindings_mut(&mut self) -> (&mut dyn InputBindings<Text>, &mut Text) {
        self.dirty_layout();
        (self.editor.get_bindings_mut(), &mut *self.text)
    }

    /// Downcasts the bindings to a concrete type and pairs them with the text document.
    pub fn get_bindings_as<B: Any>(&self) -> Option<(&B, &Text)> {
        let bindings = self.editor.get_bindings().as_any().downcast_ref::<B>()?;
        Some((bindings, &*self.text))
    }

    /// Downcasts the bindings to a concrete type and pairs them with the text document for mutation.
    pub fn get_bindings_as_mut<B: Any>(&mut self) -> Option<(&mut B, &mut Text)> {
        self.dirty_layout();
        let bindings = self.editor.get_bindings_mut().as_any_mut().downcast_mut::<B>()?;
        Some((bindings, &mut *self.text))
    }
}
