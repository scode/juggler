use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

#[derive(Debug, Clone, Copy)]
pub enum PromptAction {
    CustomDelay,
}

#[derive(Debug, Clone)]
pub struct PromptOverlay {
    pub message: String,
    pub buffer: String,
    pub action: PromptAction,
}

#[derive(Debug, Clone)]
pub enum AppMode {
    Normal,
    Prompt(PromptOverlay),
}

#[derive(Debug, Clone)]
pub struct PromptWidget {
    text: String,
}

impl PromptWidget {
    pub fn new(message: &str, buffer: &str) -> Self {
        Self {
            text: format!("{}{}", message, buffer),
        }
    }
}

impl Widget for PromptWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                let cell = &mut buf[(x, y)];
                cell.reset();
                cell.set_symbol(" ");
            }
        }

        let max_width = area.width as usize;
        let char_count = self.text.chars().count();
        let content = if char_count > max_width {
            self.text.chars().take(max_width).collect::<String>()
        } else {
            self.text
        };

        let mut x = area.x;
        let y = area.y;
        for ch in content.chars() {
            let cell = &mut buf[(x, y)];
            cell.set_symbol(ch.encode_utf8(&mut [0; 4]));
            cell.set_style(Style::default());
            x += 1;
        }
    }
}
