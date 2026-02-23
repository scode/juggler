# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.5] - 2026-02-23

### Added

- Add screenshot image at top (#235)
- Add juggler dir CLI/env overrides (#236)

### Changed

- Update screenshot (#237)
- Disable macos except on release, add linux (#238)
- Cargo update (#239)

## [0.2.4] - 2026-02-23

### Changed

- V0.2.4 (#234)

## [0.2.3] - 2026-02-22

### Changed

- Reorder installation and quick start (#231)
- Init git-cliff flow, document (#232)
- V0.2.3 (#233)

## [0.2.2] - 2026-02-22

### Changed

- Document release process. (#226)
- Document installation via Homebrew. (#227)
- Require conventional commits guidelines (#228)
- Enforce conventional commit PR titles (#229)
- V0.2.2 (#230)

## [0.2.1] - 2026-02-22

### Added

- Add impl From<&Todo> for TodoItem. (#196)
- Add SPEC.md. (#213)
- Add explicit ownership marker for Google task deletes (#217)

### Changed

- Move to more consistent error handling. (#127)
- Move constants into config.rs. (#128)
- Eliminate redonkulous dead code/not test code. (#129)
- Dependency inject reqwest clients. (#131)
- Clone() -> to_string() (#132)
- Sort items in the view layer. (#133)
- Sort deterministically in the store for diffing purposes. (#134)
- Nuke useless comments and empty lines. (#135)
- Clean up use of constants a little. (#136)
- Use oauth2 crate for less bespoke code. (#137)
- Attempt to make Windows tests only run when pushed to main. (#138)
- Cargo update (#139)
- Move some oauth related code out of google_tasks into oauth (#140)
- Cargo update (#143)
- Lightly restructure rendering logic. (#144)
- Separate UI state a little better. (#145)
- De-duplicate some filtering. (#146)
- Cargo update (#147)
- Improve CI caching. (#148)
- Move external editor based item editing out of store.rs. (#149)
- Privatize function. (#150)
- Revert previous release CI changes. (#151)
- Improve due date readability slightly. (#152)
- Factor into helper: pick_juggler_list (#153)
- Move orphan deletion into a helper function. (#154)
- Move task diff logging into helper. (#155)
- Split UI into modules. (#164)
- Cargo update (#165)
- Cargo update (#171)
- Cargo update (#175)
- De-duplicate some HTTP error handling. (#176)
- De-duplicate pagination. (#177)
- Make j: prefix a constant. (#178)
- De-dupe navigation logic. (#179)
- De-dupe selection logic. (#180)
- De-dupe urgency color selection. (#181)
- De-dupe oauth client creation. (#182)
- Some unwrap() -> expect(). (#183)
- De-dupe TODO building in tests. (#184)
- De-dupe desired task values. (#185)
- Simplify error handling in sync command. (#186)
- Extract oauth_err helper for error wrapping. (#187)
- Extract sorted_indices helper for index collection. (#188)
- Replace unwrap() with expect() for midnight time. (#189)
- Extract DUE_SOON_THRESHOLD_SECS constant. (#190)
- Extract DEFAULT_TOKEN_EXPIRY_SECS constant. (#191)
- Use debug logging for keyring operations. (#192)
- Extract GoogleTask::from_desired constructor. (#193)
- Deduplicate test_clock() helper. (#194)
- Define COMMENT_INDENT constant for expanded comment lines. (#197)
- Refactor display_text_internal to handle expansion directly. (#198)
- Fix test fixture path to use CARGO_MANIFEST_DIR. (#201)
- Fix archive timestamp collision with counter suffix. (#200)
- Fix UTF-8 truncation bug in PromptWidget. (#202)
- Cargo update (#203)
- Improve EDITOR support. (#206)
- Fix dry-run local persistence side effects (#207)
- Validate OAuth callback state (#208)
- Centralize normal-mode key bindings (#209)
- Refactor model/update/view pipeline with inline side effects (#210)
- Cargo update (#211)
- Cargo update (#212)
- Consolidate agent policy docs (#214)
- Enforce non-empty titles for edited todos (#215)
- Block quit-with-sync when local save fails (#216)
- Make logout idempotent when no token exists (#218)
- Switch TODO storage to TOML with migration (#220)
- Move Google sync docs to dedicated page (#222)
- Set up cargo dist based homebrew tap publishing. (#223)
- Upgrade to latest cargo dist (#224)
- 0.2.1 (#225)

### Removed

- Remove some use of sync Mutex in async context. (#130)
- Remove test-only collapsed_summary method. (#195)
- Remove unused terminal parameter from handle_custom_delay. (#199)
- Remove j-prefix migration path for ownership marker (#219)
- Remove remaining YAML todo support (#221)

## [0.1.2] - 2025-09-26

### Changed

- Cargo update
- 0.1.2

## [0.1.1] - 2025-09-26

### Changed

- Do not try to build releases for ARM. Some limitation around private repos. (#123)

## [0.1.0] - 2025-09-26

### Added

- Add AGENTS.md
- Add expandable todo comments (#6)
- Add bottom help text and bar (#15)
- Add basic done/not done split. (#23)
- Support editing an item via text editor.
- Support editing a TODO item via text editor.
- Support basic syncing of items to Google Tasks. (#27)
- Add support for refresh token authentication (#34)
- Support todo item creation by launching editor. (#45)
- Add 't' to allow typing a fixed relative due date. (#69)
- Add test coverage for dry run. (#107)

### Changed

- Initial commit
- Initial app based on cargo generate ratatui/templates simple
- Counter tutorial.
- Basic list.
- Select first item by default.
- Load TODO items from YAML file (#4)
- Show '>' marker for todos with comments (#7)
- Indent expanded comments (#8)
- Adjust cursor prefix for expanded comments (#13)
- Cargo update (#18)
- Use constants for key codes. (#19)
- Do not silently ignore YAML loading errors. (#20)
- Improve visual indicators. (#21)
- Use standard clippy check (#22)
- CLAUDE.md
- Have DONE section take less screen space.
- Use strike-through for DONE items.
- Introduce task selection (currently NOOP). (#24)
- Drop use of emoji indicators - they are bugging out claude code :)
- Selection affects done toggling. (#25)
- Indent expanded item descriptions by 3 more spaces.
- Collapse item when they are marked done.
- Basic due date handling.
- Sort by due date.
- Due date padding and urgency coloring.
- S/s to snooze
- TodoConfig -> TodoItem
- Move TodoItem into store namespace.
- UI -> ui.rs
- Losen version requirements, cargo update (#26)
- Snoozing is relative to current due date except when in the past.
- Move TODO loading into store.rs.
- Fix selection - now separates pending and done items.
- Save TODOs on exit.
- Handle non-existence of TODOs.yaml.
- Update CLAUDE.md
- Stronger admonishment to run fmt/clippy
- Use proper logging for older log statements. (#28)
- Use INFO log level by default. (#29)
- Move to browser based refresh token flow. (#35)
- Use open crate for launching browser (#36)
- Centralize config constants (#37)
- Google tasks (claude code) (#39)
- Refresh CLAUDE.md (#41)
- Cargo update (#46)
- Move TODOs into ~/.juggler (#47)
- Update AGENTS.md with project overview (#55)
- AGENTS.md and code clippy fixes (#56)
- Embed OAuth client secret. (#59)
- Cargo update (#60)
- Log item changes during google tasks sync (#61)
- Fix 'x' button functionality (#62)
- Fix relative date display for todo items (#63)
- Fix spurious syncing of tasks due to perceived due date changes. (#65)
- Setup rust environment and add to profile (#67)
- Improve snoozing, add postpone (#66)
- Make cursor environment actual valid JSON duh. Sad face for readability. (#68)
- .cursor/environment.json - another attempted fix
- Fix confusing use of "selected" for two things. (#70)
- Update README to better reflect current state of juggler.
- Comment indicator is now (...) and we indent it more when open. (#72)
- Fix formatting. (#73)
- Change "due soon" cut-off to 48 hours. (#74)
- Paginate when enumerating lists and items. (#79)
- Preserve item selection when snoozing/postponing. (#82)
- Refactor test - split due date choice testing from selection preservation. (#83)
- Use a mockable clock in the UI. (#84)
- Cargo update (#88)
- Switch to keychain storage and remove explicit refresh token management. (#89)
- Make a credential store trait that is dependency injected. (#92)
- Log before keyring access. (#94)
- Sync-on-quit triggered by quitting using 'Q' (capital q). (#93)
- Cargo update (#95)
- Use mockable time for google tasks sync. (#97)
- More use of mockable clock. Should be the last. Fixes #81. (#98)
- Bump actions/checkout from 4 to 5 (#86)
- Improve logging setup. (#102)
- Trigger on all pull requests (#105)
- De-duplication in date handling. (#103)
- Use more restrictive permissions. (#104)
- Fix fmt. (#106)
- Improve README w.r.t. authentication. (#108)
- Do not update local task ids when in dry-run mode. (#109)
- Improve AGENTS.md. (#110)
- Avoid unofficial github actions. (#111)
- Ci - only run windows tests on push to main (#114)
- Revert previous. (#115)
- Cargo update (#116)
- Has_comment - de-duplicate, use prod path intests. (#117)
- Use temp file correctly during yaml save. (#119)
- Cargo update
- Set up (hopefully working) release binary builds using dist. (#122)

### Removed

- Remove highlighting of items under cursor.
- Remove hardcoded path
- Remove spammy log message (#30)
- Remove some code duplication. (#118)

[0.2.5]: https://github.com/scode/juggler/compare/v0.2.4..v0.2.5
[0.2.4]: https://github.com/scode/juggler/compare/v0.2.3..v0.2.4
[0.2.3]: https://github.com/scode/juggler/compare/v0.2.2..v0.2.3
[0.2.2]: https://github.com/scode/juggler/compare/v0.2.1..v0.2.2
[0.2.1]: https://github.com/scode/juggler/compare/v0.1.2..v0.2.1
[0.1.2]: https://github.com/scode/juggler/compare/v0.1.1..v0.1.2
[0.1.1]: https://github.com/scode/juggler/compare/v0.1.0..v0.1.1
[0.1.0]: https://github.com/scode/juggler/tree/v0.1.0

<!-- generated by git-cliff -->
