use proptest::prelude::*;

// Parsing arbitrary bytes should never panic — it should return an error.
proptest! {
    #[test]
    fn parse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = replay_core::parse(&data);
    }

    #[test]
    fn parse_short_inputs_are_errors(data in proptest::collection::vec(any::<u8>(), 0..30)) {
        let result = replay_core::parse(&data);
        prop_assert!(result.is_err());
    }

    #[test]
    fn parse_commands_never_panics(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let (_, w) = replay_core::command::parse_commands_with_warnings(&data);
        prop_assert!(w.skipped_bytes as usize <= data.len());
    }

    #[test]
    fn parse_extra_sections_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = replay_core::extra::parse_extra_sections(
            &data,
            0,
            replay_core::format::Format::Modern121,
        );
    }

    #[test]
    fn normalize_map_name_never_panics(s in ".*") {
        let _ = replay_core::metadata::normalize_map_name(&s);
    }

    #[test]
    fn normalize_player_name_never_panics(s in ".*") {
        let _ = replay_core::identity::normalize_name(&s);
    }

    #[test]
    fn similarity_is_symmetric(
        a in proptest::collection::vec(0u16..200, 0..50),
        b in proptest::collection::vec(0u16..200, 0..50),
    ) {
        use replay_core::analysis::BuildAction;
        use replay_core::analysis::BuildOrderEntry;
        use replay_core::similarity::{BuildSequence, similarity};

        let entries_a: Vec<BuildOrderEntry> = a.iter().enumerate().map(|(i, &id)| {
            BuildOrderEntry {
                frame: i as u32 * 100,
                real_seconds: i as f64 * 4.2,
                player_id: 0,
                action: BuildAction::Train(id),
            }
        }).collect();
        let entries_b: Vec<BuildOrderEntry> = b.iter().enumerate().map(|(i, &id)| {
            BuildOrderEntry {
                frame: i as u32 * 100,
                real_seconds: i as f64 * 4.2,
                player_id: 0,
                action: BuildAction::Train(id),
            }
        }).collect();

        let seq_a = BuildSequence::from_build_order(&entries_a, 0);
        let seq_b = BuildSequence::from_build_order(&entries_b, 0);

        let s_ab = similarity(&seq_a, &seq_b);
        let s_ba = similarity(&seq_b, &seq_a);

        prop_assert!((s_ab - s_ba).abs() < 0.001, "similarity not symmetric: {} vs {}", s_ab, s_ba);
        prop_assert!((0.0..=1.0).contains(&s_ab), "similarity out of range: {}", s_ab);
    }

    #[test]
    fn classify_empty_never_panics(race_idx in 0u8..4) {
        use replay_core::header::Race;
        let race = match race_idx {
            0 => Race::Terran,
            1 => Race::Protoss,
            2 => Race::Zerg,
            _ => Race::Unknown(99),
        };
        let _ = replay_core::classify::classify_opening(&[], 0, &race);
    }

    #[test]
    fn detect_phases_empty_never_panics(total_frames in 0u32..100000) {
        let _ = replay_core::phases::detect_phases(&[], total_frames);
    }
}
