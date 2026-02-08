use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect};

    #[test]
    fn prompt_widget_clears_area_and_renders_text() {
        let area = Rect::new(0, 0, 20, 2);
        let mut buf = Buffer::empty(area);

        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_symbol("X");
            }
        }

        let message = "Prompt: ";
        let input = "abc";
        PromptWidget::new(message, input).render(area, &mut buf);

        let line0: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        let expected_content = format!("{}{}", message, input);
        let mut expected_line0 = expected_content.clone();
        if expected_line0.len() < area.width as usize {
            expected_line0.push_str(&" ".repeat(area.width as usize - expected_line0.len()));
        } else {
            expected_line0.truncate(area.width as usize);
        }
        assert_eq!(line0, expected_line0);

        let line1: String = (0..area.width)
            .map(|x| buf[(x, area.y + 1)].symbol())
            .collect();
        assert_eq!(line1, " ".repeat(area.width as usize));
    }

    #[test]
    fn prompt_widget_truncates_to_width() {
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);

        PromptWidget::new("Hello", "World").render(area, &mut buf);

        let line: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        assert_eq!(line, "Hello");
    }

    #[test]
    fn prompt_widget_handles_multibyte_utf8() {
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);

        PromptWidget::new("", "café!").render(area, &mut buf);

        let line: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        assert_eq!(line, "café!");
    }

    #[test]
    fn prompt_widget_truncates_multibyte_utf8() {
        let area = Rect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);

        PromptWidget::new("", "café").render(area, &mut buf);

        let line: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        assert_eq!(line, "caf");
    }
}
