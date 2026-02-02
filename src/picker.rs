use std::collections::BTreeSet;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct PickerState {
    pub is_open: bool,
    pub options: Vec<String>,
    pub selected: Option<usize>,
    pub active: BTreeSet<String>,
}

impl PickerState {
    pub fn from_options(options: Vec<String>) -> Self {
        let mut state = Self::default();
        state.set_options(options);
        state
    }

    pub fn set_options<I: IntoIterator<Item = String>>(&mut self, options: I) {
        let unique: BTreeSet<String> = options.into_iter().collect();
        self.options = unique.into_iter().collect();
        self.clamp_selection();
    }

    pub fn toggle_open(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.clamp_selection();
        } else {
            self.selected = None;
        }
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.selected = None;
    }

    pub fn move_selection(&mut self, direction: isize) {
        if self.options.is_empty() {
            self.selected = None;
            return;
        }
        // If nothing selected, start at the logical "position before first item"
        // so that moving forward selects the first item
        let current = self.selected.map(|i| i as isize).unwrap_or(-1);
        let next = (current + direction).clamp(0, self.options.len() as isize - 1);
        self.selected = Some(next as usize);
    }

    pub fn toggle_active(&mut self) {
        if let Some(idx) = self.selected {
            if let Some(value) = self.options.get(idx).cloned() {
                if self.active.contains(&value) {
                    self.active.remove(&value);
                } else {
                    self.active.insert(value);
                }
            }
        }
    }

    pub fn clear_active(&mut self) {
        self.active.clear();
    }

    pub fn set_selected_to_value(&mut self, value: &str) {
        self.selected = self.options.iter().position(|v| v == value);
    }

    fn clamp_selection(&mut self) {
        if self.options.is_empty() {
            self.selected = None;
            return;
        }
        let max_idx = self.options.len() - 1;
        let selection = self.selected.unwrap_or(0).min(max_idx);
        self.selected = Some(selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_open() {
        let mut picker = PickerState::default();
        assert!(!picker.is_open);
        picker.toggle_open();
        assert!(picker.is_open);
        picker.toggle_open();
        assert!(!picker.is_open);
    }

    #[test]
    fn test_toggle_open_resets_selection_when_closing() {
        let mut picker = PickerState::from_options(vec!["A".to_string(), "B".to_string()]);
        picker.toggle_open();
        picker.selected = Some(1);
        picker.toggle_open();
        assert!(!picker.is_open);
        assert_eq!(picker.selected, None);
    }

    #[test]
    fn test_move_selection_with_options() {
        let mut picker = PickerState::from_options(vec!["A".to_string(), "B".to_string()]);
        // from_options calls clamp_selection which sets selected to Some(0)
        assert_eq!(picker.selected, Some(0));
        picker.move_selection(1);
        assert_eq!(picker.selected, Some(1));
        picker.move_selection(1);
        assert_eq!(picker.selected, Some(1)); // Stays at last
    }

    #[test]
    fn test_move_selection_backward() {
        let mut picker =
            PickerState::from_options(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        picker.selected = Some(2);
        picker.move_selection(-1);
        assert_eq!(picker.selected, Some(1));
        picker.move_selection(-1);
        assert_eq!(picker.selected, Some(0));
        picker.move_selection(-1);
        assert_eq!(picker.selected, Some(0));
    }

    #[test]
    fn test_move_selection_empty_options() {
        let mut picker = PickerState::default();
        picker.move_selection(1);
        assert_eq!(picker.selected, None);
    }

    #[test]
    fn test_toggle_active_adds_and_removes() {
        let mut picker = PickerState::from_options(vec!["A".to_string(), "B".to_string()]);
        picker.selected = Some(0);
        picker.toggle_active();
        assert!(picker.active.contains("A"));
        assert_eq!(picker.active.len(), 1);
        picker.toggle_active();
        assert!(!picker.active.contains("A"));
        assert!(picker.active.is_empty());
    }

    #[test]
    fn test_toggle_active_no_selection_does_nothing() {
        let mut picker = PickerState::from_options(vec!["A".to_string()]);
        picker.selected = None;
        picker.toggle_active();
        assert!(picker.active.is_empty());
    }

    #[test]
    fn test_clear_active() {
        let mut picker = PickerState::from_options(vec!["A".to_string(), "B".to_string()]);
        picker.selected = Some(0);
        picker.toggle_active();
        picker.selected = Some(1);
        picker.toggle_active();
        assert_eq!(picker.active.len(), 2);
        picker.clear_active();
        assert!(picker.active.is_empty());
    }

    #[test]
    fn test_set_selected_to_value() {
        let mut picker =
            PickerState::from_options(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        picker.set_selected_to_value("B");
        assert_eq!(picker.selected, Some(1));
    }

    #[test]
    fn test_set_selected_to_value_not_found() {
        let mut picker = PickerState::from_options(vec!["A".to_string(), "B".to_string()]);
        picker.set_selected_to_value("Z");
        assert_eq!(picker.selected, None);
    }

    #[test]
    fn test_clamp_selection_bounds() {
        let mut picker = PickerState::from_options(vec!["A".to_string()]);
        picker.selected = Some(5);
        picker.clamp_selection();
        assert_eq!(picker.selected, Some(0));
    }

    #[test]
    fn test_from_options_dedupes() {
        let picker = PickerState::from_options(vec![
            "A".to_string(),
            "B".to_string(),
            "A".to_string(),
            "C".to_string(),
            "B".to_string(),
        ]);
        assert_eq!(picker.options, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_set_options_updates_selection() {
        let mut picker = PickerState::default();
        picker.selected = Some(5);
        picker.set_options(vec!["X".to_string(), "Y".to_string()]);
        assert_eq!(picker.options, vec!["X", "Y"]);
        assert_eq!(picker.selected, Some(1));
    }

    #[test]
    fn test_close() {
        let mut picker = PickerState::from_options(vec!["A".to_string()]);
        picker.toggle_open();
        picker.selected = Some(0);
        picker.close();
        assert!(!picker.is_open);
        assert_eq!(picker.selected, None);
    }
}
