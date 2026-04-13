const MAX_PLAYERS: usize = 8;
const MAX_SELECTION: usize = 12;
const HOTKEY_GROUPS: usize = 10;

/// Per-player unit selection and hotkey group tracking.
#[derive(Debug, Clone)]
pub struct SelectionState {
    selections: [Vec<u16>; MAX_PLAYERS],
    hotkeys: [[Vec<u16>; HOTKEY_GROUPS]; MAX_PLAYERS],
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selections: std::array::from_fn(|_| Vec::new()),
            hotkeys: std::array::from_fn(|_| std::array::from_fn(|_| Vec::new())),
        }
    }
}

impl SelectionState {
    pub fn set_selection(&mut self, player: u8, tags: &[u16]) {
        if let Some(sel) = self.selections.get_mut(player as usize) {
            sel.clear();
            sel.extend_from_slice(&tags[..tags.len().min(MAX_SELECTION)]);
        }
    }

    pub fn add_to_selection(&mut self, player: u8, tags: &[u16]) {
        if let Some(sel) = self.selections.get_mut(player as usize) {
            for &tag in tags {
                if !sel.contains(&tag) && sel.len() < MAX_SELECTION {
                    sel.push(tag);
                }
            }
        }
    }

    pub fn remove_from_selection(&mut self, player: u8, tags: &[u16]) {
        if let Some(sel) = self.selections.get_mut(player as usize) {
            sel.retain(|t| !tags.contains(t));
        }
    }

    pub fn assign_hotkey(&mut self, player: u8, group: u8) {
        if (player as usize) < MAX_PLAYERS && (group as usize) < HOTKEY_GROUPS {
            let tags = self.selections[player as usize].clone();
            self.hotkeys[player as usize][group as usize] = tags;
        }
    }

    pub fn recall_hotkey(&mut self, player: u8, group: u8) {
        if (player as usize) < MAX_PLAYERS && (group as usize) < HOTKEY_GROUPS {
            self.selections[player as usize] =
                self.hotkeys[player as usize][group as usize].clone();
        }
    }

    pub fn selected_tags(&self, player: u8) -> &[u16] {
        self.selections
            .get(player as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_selection() {
        let mut sel = SelectionState::default();
        sel.set_selection(0, &[1, 2, 3]);
        assert_eq!(sel.selected_tags(0), &[1, 2, 3]);
    }

    #[test]
    fn test_add_to_selection() {
        let mut sel = SelectionState::default();
        sel.set_selection(0, &[1, 2]);
        sel.add_to_selection(0, &[3, 4]);
        assert_eq!(sel.selected_tags(0), &[1, 2, 3, 4]);
    }

    #[test]
    fn test_add_no_duplicates() {
        let mut sel = SelectionState::default();
        sel.set_selection(0, &[1, 2]);
        sel.add_to_selection(0, &[2, 3]);
        assert_eq!(sel.selected_tags(0), &[1, 2, 3]);
    }

    #[test]
    fn test_remove_from_selection() {
        let mut sel = SelectionState::default();
        sel.set_selection(0, &[1, 2, 3, 4]);
        sel.remove_from_selection(0, &[2, 4]);
        assert_eq!(sel.selected_tags(0), &[1, 3]);
    }

    #[test]
    fn test_hotkey_assign_recall() {
        let mut sel = SelectionState::default();
        sel.set_selection(0, &[10, 20, 30]);
        sel.assign_hotkey(0, 1);

        // Change selection
        sel.set_selection(0, &[99]);
        assert_eq!(sel.selected_tags(0), &[99]);

        // Recall hotkey 1
        sel.recall_hotkey(0, 1);
        assert_eq!(sel.selected_tags(0), &[10, 20, 30]);
    }

    #[test]
    fn test_max_selection() {
        let mut sel = SelectionState::default();
        let tags: Vec<u16> = (0..20).collect();
        sel.set_selection(0, &tags);
        assert_eq!(sel.selected_tags(0).len(), MAX_SELECTION);
    }

    #[test]
    fn test_invalid_player() {
        let sel = SelectionState::default();
        assert_eq!(sel.selected_tags(99), &[] as &[u16]);
    }
}
