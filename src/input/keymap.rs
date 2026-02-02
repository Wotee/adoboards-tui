//! Key mapping and matching utilities
//!
//! This module provides utilities for matching key sequences against configured key bindings.

use crossterm::event::KeyCode;

/// Check if a key press matches a target sequence
///
/// Supports both single-key sequences (e.g., "q") and two-key sequences (e.g., "gg").
/// For two-key sequences, the last_key parameter is checked to see if the first key
/// was pressed previously.
///
/// # Examples
///
/// ```
/// use crossterm::event::KeyCode;
/// use crate::input::keymap::key_matches_sequence;
///
/// // Single key match
/// assert!(key_matches_sequence('q', None, "q"));
/// assert!(!key_matches_sequence('a', None, "q"));
///
/// // Two-key sequence match
/// let last_g = Some(KeyCode::Char('g'));
/// assert!(key_matches_sequence('g', last_g, "gg"));
///
/// // Two-key sequence no match (wrong first key)
/// let last_a = Some(KeyCode::Char('a'));
/// assert!(!key_matches_sequence('g', last_a, "gg"));
/// ```
pub fn key_matches_sequence(
    current_key: char,
    last_key: Option<KeyCode>,
    target_sequence: &str,
) -> bool {
    if target_sequence.len() == 2 {
        let first_char = target_sequence.chars().next().unwrap();
        let second_char = target_sequence.chars().nth(1).unwrap();
        return last_key == Some(KeyCode::Char(first_char)) && current_key == second_char;
    }

    if target_sequence.len() == 1 {
        return target_sequence.chars().next() == Some(current_key);
    }

    false
}

/// A matcher that tracks key sequence state
///
/// This struct helps track the last key pressed for multi-key sequence matching.
#[derive(Debug, Default)]
pub struct KeyMatcher {
    last_key: Option<KeyCode>,
}

impl KeyMatcher {
    /// Create a new KeyMatcher with no last key
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the current key matches a target sequence
    ///
    /// This uses the internally tracked last_key for sequence matching.
    pub fn matches(&self, current_key: char, target_sequence: &str) -> bool {
        key_matches_sequence(current_key, self.last_key, target_sequence)
    }

    /// Update the tracked last key
    ///
    /// Call this after processing a key to update the sequence state.
    pub fn update(&mut self, key: KeyCode) {
        self.last_key = Some(key);
    }

    /// Reset the tracked last key
    pub fn reset(&mut self) {
        self.last_key = None;
    }

    /// Get the currently tracked last key
    pub fn last_key(&self) -> Option<KeyCode> {
        self.last_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_key_match() {
        assert!(key_matches_sequence('q', None, "q"));
        assert!(key_matches_sequence('a', None, "a"));
    }

    #[test]
    fn test_single_key_no_match() {
        assert!(!key_matches_sequence('a', None, "q"));
        assert!(!key_matches_sequence('q', None, "a"));
    }

    #[test]
    fn test_two_key_sequence_match() {
        let last_g = Some(KeyCode::Char('g'));
        assert!(key_matches_sequence('g', last_g, "gg"));

        let last_d = Some(KeyCode::Char('d'));
        assert!(key_matches_sequence('d', last_d, "dd"));
    }

    #[test]
    fn test_two_key_sequence_no_match_wrong_first() {
        let last_a = Some(KeyCode::Char('a'));
        assert!(!key_matches_sequence('g', last_a, "gg"));
    }

    #[test]
    fn test_two_key_sequence_no_match_wrong_second() {
        let last_g = Some(KeyCode::Char('g'));
        assert!(!key_matches_sequence('a', last_g, "gg"));
    }

    #[test]
    fn test_two_key_sequence_no_match_no_last() {
        assert!(!key_matches_sequence('g', None, "gg"));
    }

    #[test]
    fn test_empty_sequence_returns_false() {
        assert!(!key_matches_sequence('q', None, ""));
    }

    #[test]
    fn test_long_sequence_returns_false() {
        // Currently only supports 1 or 2 character sequences
        assert!(!key_matches_sequence('g', None, "ggg"));
    }

    #[test]
    fn test_key_matcher_single_key() {
        let matcher = KeyMatcher::new();
        assert!(matcher.matches('q', "q"));
        assert!(!matcher.matches('a', "q"));
    }

    #[test]
    fn test_key_matcher_sequence() {
        let mut matcher = KeyMatcher::new();

        // First 'g' press
        assert!(!matcher.matches('g', "gg"));
        matcher.update(KeyCode::Char('g'));

        // Second 'g' press with tracked last key
        assert!(matcher.matches('g', "gg"));
    }

    #[test]
    fn test_key_matcher_reset() {
        let mut matcher = KeyMatcher::new();

        // Press 'g' and track it
        matcher.update(KeyCode::Char('g'));
        assert!(matcher.matches('g', "gg"));

        // Reset and try again
        matcher.reset();
        assert!(!matcher.matches('g', "gg"));
    }

    #[test]
    fn test_key_matcher_last_key_tracking() {
        let mut matcher = KeyMatcher::new();
        assert_eq!(matcher.last_key(), None);

        matcher.update(KeyCode::Char('x'));
        assert_eq!(matcher.last_key(), Some(KeyCode::Char('x')));

        matcher.update(KeyCode::Char('y'));
        assert_eq!(matcher.last_key(), Some(KeyCode::Char('y')));
    }

    #[test]
    fn test_key_matcher_non_char_keys_not_tracked_in_match() {
        // This tests that we properly handle KeyCode variants that aren't Char
        // When last_key is Some(Enter), it shouldn't match 'g' in "gg"
        let matcher_with_enter = KeyMatcher {
            last_key: Some(KeyCode::Enter),
        };
        assert!(!matcher_with_enter.matches('g', "gg"));
    }
}
