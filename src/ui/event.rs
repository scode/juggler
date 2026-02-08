use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::error::Result;

use super::keymap::action_for_key;
use super::model::AppMode;
use super::update::Action;

pub(super) fn map_key(mode: &AppMode, key: KeyEvent) -> Option<Action> {
    match mode {
        AppMode::Normal => map_normal_mode_key(key),
        AppMode::Prompt(_) => map_prompt_mode_key(key),
    }
}

fn map_normal_mode_key(key: KeyEvent) -> Option<Action> {
    if !matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
        return None;
    }
    action_for_key(key.code).map(Action::Normal)
}

fn map_prompt_mode_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::PromptSubmit),
        KeyCode::Esc => Some(Action::PromptCancel),
        KeyCode::Backspace => Some(Action::PromptBackspace),
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            Some(Action::PromptInput(c))
        }
        _ => None,
    }
}

pub(super) fn read_action(mode: &AppMode) -> Result<Option<Action>> {
    match event::read()? {
        Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
            Ok(map_key(mode, key_event))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::keymap::{Action as NormalAction, key_for_action};
    use crate::ui::model::{PromptAction, PromptOverlay};

    #[test]
    fn normal_mode_maps_bound_keys() {
        let key = KeyEvent::new(key_for_action(NormalAction::ToggleDone), KeyModifiers::NONE);
        assert!(matches!(
            map_key(&AppMode::Normal, key),
            Some(Action::Normal(NormalAction::ToggleDone))
        ));
    }

    #[test]
    fn normal_mode_ignores_unknown_keys() {
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert!(map_key(&AppMode::Normal, key).is_none());
    }

    #[test]
    fn normal_mode_ignores_control_modified_keys() {
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        assert!(map_key(&AppMode::Normal, key).is_none());
    }

    #[test]
    fn prompt_mode_maps_submit_cancel_and_edit_keys() {
        let mode = AppMode::Prompt(PromptOverlay {
            message: "Delay: ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });

        assert!(matches!(
            map_key(&mode, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::PromptSubmit)
        ));
        assert!(matches!(
            map_key(&mode, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::PromptCancel)
        ));
        assert!(matches!(
            map_key(&mode, KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(Action::PromptBackspace)
        ));
        assert!(matches!(
            map_key(&mode, KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
            Some(Action::PromptInput('d'))
        ));
    }

    #[test]
    fn prompt_mode_ignores_control_modified_characters() {
        let mode = AppMode::Prompt(PromptOverlay {
            message: "Delay: ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });
        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert!(map_key(&mode, key).is_none());
    }
}
