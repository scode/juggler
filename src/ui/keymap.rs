use std::sync::LazyLock;

use crossterm::event::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Quit,
    QuitWithSync,
    ToggleExpand,
    NextItem,
    PreviousItem,
    ToggleDone,
    Edit,
    ToggleSelect,
    SnoozeDay,
    UnsnoozeDay,
    PostponeWeek,
    PreponeWeek,
    Create,
    CustomDelay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KeyBinding {
    pub(super) action: Action,
    pub(super) key_code: KeyCode,
    pub(super) help_token: &'static str,
}

const HELP_SUFFIX: &str = "Ops affect selected; if none, the cursored item.";

const KEY_BINDINGS: [KeyBinding; 14] = [
    KeyBinding {
        action: Action::ToggleExpand,
        key_code: KeyCode::Char('o'),
        help_token: "o-open",
    },
    KeyBinding {
        action: Action::NextItem,
        key_code: KeyCode::Char('j'),
        help_token: "j/k-nav",
    },
    KeyBinding {
        action: Action::PreviousItem,
        key_code: KeyCode::Char('k'),
        help_token: "j/k-nav",
    },
    KeyBinding {
        action: Action::ToggleSelect,
        key_code: KeyCode::Char('x'),
        help_token: "x-select",
    },
    KeyBinding {
        action: Action::ToggleDone,
        key_code: KeyCode::Char('e'),
        help_token: "e-done",
    },
    KeyBinding {
        action: Action::Edit,
        key_code: KeyCode::Char('E'),
        help_token: "E-edit",
    },
    KeyBinding {
        action: Action::Create,
        key_code: KeyCode::Char('c'),
        help_token: "c-new",
    },
    KeyBinding {
        action: Action::SnoozeDay,
        key_code: KeyCode::Char('s'),
        help_token: "s:+1d",
    },
    KeyBinding {
        action: Action::UnsnoozeDay,
        key_code: KeyCode::Char('S'),
        help_token: "S:-1d",
    },
    KeyBinding {
        action: Action::PostponeWeek,
        key_code: KeyCode::Char('p'),
        help_token: "p:+7d",
    },
    KeyBinding {
        action: Action::PreponeWeek,
        key_code: KeyCode::Char('P'),
        help_token: "P:-7d",
    },
    KeyBinding {
        action: Action::CustomDelay,
        key_code: KeyCode::Char('t'),
        help_token: "t-custom",
    },
    KeyBinding {
        action: Action::Quit,
        key_code: KeyCode::Char('q'),
        help_token: "q-quit",
    },
    KeyBinding {
        action: Action::QuitWithSync,
        key_code: KeyCode::Char('Q'),
        help_token: "Q-quit+sync",
    },
];

static HELP_TEXT: LazyLock<String> = LazyLock::new(|| {
    let mut tokens: Vec<&'static str> = Vec::new();
    for binding in KEY_BINDINGS {
        if !tokens.contains(&binding.help_token) {
            tokens.push(binding.help_token);
        }
    }

    let joined_tokens = tokens.join(", ");
    format!("{joined_tokens}. {HELP_SUFFIX}")
});

pub(super) fn action_for_key(key_code: KeyCode) -> Option<Action> {
    KEY_BINDINGS
        .iter()
        .find(|binding| binding.key_code == key_code)
        .map(|binding| binding.action)
}

#[cfg(test)]
pub(super) fn key_for_action(action: Action) -> KeyCode {
    KEY_BINDINGS
        .iter()
        .find(|binding| binding.action == action)
        .map(|binding| binding.key_code)
        .expect("all actions must have a key binding")
}

pub(super) fn help_text() -> &'static str {
    HELP_TEXT.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_actions_round_trip_through_key_lookup() {
        let all_actions = [
            Action::Quit,
            Action::QuitWithSync,
            Action::ToggleExpand,
            Action::NextItem,
            Action::PreviousItem,
            Action::ToggleDone,
            Action::Edit,
            Action::ToggleSelect,
            Action::SnoozeDay,
            Action::UnsnoozeDay,
            Action::PostponeWeek,
            Action::PreponeWeek,
            Action::Create,
            Action::CustomDelay,
        ];

        for action in all_actions {
            assert_eq!(action_for_key(key_for_action(action)), Some(action));
        }
    }

    #[test]
    fn key_bindings_have_unique_key_codes() {
        let mut seen: Vec<KeyCode> = Vec::new();

        for binding in KEY_BINDINGS {
            assert!(
                !seen.contains(&binding.key_code),
                "duplicate key binding for {:?}",
                binding.key_code
            );
            seen.push(binding.key_code);
        }
    }

    #[test]
    fn help_text_matches_expected_footer() {
        assert_eq!(
            help_text(),
            "o-open, j/k-nav, x-select, e-done, E-edit, c-new, s:+1d, S:-1d, p:+7d, P:-7d, t-custom, q-quit, Q-quit+sync. Ops affect selected; if none, the cursored item."
        );
    }
}
