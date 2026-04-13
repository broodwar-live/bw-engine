/// A single command issued by a player at a specific game frame.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameCommand {
    pub frame: u32,
    pub player_id: u8,
    pub command: Command,
}

/// Decoded command types.
///
/// Only gameplay-relevant commands are fully decoded. UI-only commands
/// (lobby, voice, sync) are captured as `Other` with their type ID.
#[derive(Debug, Clone, serde::Serialize)]
pub enum Command {
    Select {
        unit_tags: Vec<u16>,
    },
    SelectAdd {
        unit_tags: Vec<u16>,
    },
    SelectRemove {
        unit_tags: Vec<u16>,
    },
    Build {
        order: u8,
        x: u16,
        y: u16,
        unit_type: u16,
    },
    RightClick {
        x: u16,
        y: u16,
        target_tag: u16,
        unit_type: u16,
        queued: bool,
    },
    TargetedOrder {
        x: u16,
        y: u16,
        target_tag: u16,
        unit_type: u16,
        order: u8,
        queued: bool,
    },
    Hotkey {
        action: HotkeyAction,
        group: u8,
    },
    Train {
        unit_type: u16,
    },
    CancelTrain {
        unit_tag: u16,
    },
    UnitMorph {
        unit_type: u16,
    },
    BuildingMorph {
        unit_type: u16,
    },
    Research {
        tech_type: u8,
    },
    CancelResearch,
    Upgrade {
        upgrade_type: u8,
    },
    CancelUpgrade,
    Stop {
        queued: bool,
    },
    HoldPosition {
        queued: bool,
    },
    Siege {
        queued: bool,
    },
    Unsiege {
        queued: bool,
    },
    Cloak {
        queued: bool,
    },
    Decloak {
        queued: bool,
    },
    Burrow {
        queued: bool,
    },
    Unburrow {
        queued: bool,
    },
    ReturnCargo {
        queued: bool,
    },
    UnloadAll {
        queued: bool,
    },
    Unload {
        unit_tag: u16,
    },
    LiftOff {
        x: u16,
        y: u16,
    },
    MergeArchon,
    MergeDarkArchon,
    TrainFighter,
    Stim,
    CancelBuild,
    CancelMorph,
    CancelAddon,
    CancelNuke,
    MinimapPing {
        x: u16,
        y: u16,
    },
    Chat {
        sender_slot: u8,
        message: String,
    },
    LeaveGame {
        reason: u8,
    },
    KeepAlive,
    /// Latency, sync, lobby, voice, and other non-gameplay commands.
    Other {
        type_id: u8,
    },
}

/// Hotkey assign vs recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HotkeyAction {
    Assign,
    Select,
}

impl Command {
    /// Whether this command represents a meaningful player action for APM counting.
    /// Excludes keepalive, selections, hotkey recalls, and non-gameplay commands.
    pub fn is_meaningful_action(&self) -> bool {
        !matches!(
            self,
            Command::KeepAlive
                | Command::Other { .. }
                | Command::Select { .. }
                | Command::SelectAdd { .. }
                | Command::SelectRemove { .. }
                | Command::Chat { .. }
                | Command::LeaveGame { .. }
                | Command::MinimapPing { .. }
        )
    }

    /// Whether this command represents an "effective" action for EAPM.
    /// Stricter than meaningful: also excludes hotkey recalls and redundant stops.
    pub fn is_effective_action(&self) -> bool {
        if !self.is_meaningful_action() {
            return false;
        }
        // Hotkey recall (select) is not effective — only assign is.
        if let Command::Hotkey { action, .. } = self {
            return *action == HotkeyAction::Assign;
        }
        true
    }

    /// Whether this command affects the build order (structure, unit, tech, upgrade).
    pub fn is_build_order_action(&self) -> bool {
        matches!(
            self,
            Command::Build { .. }
                | Command::Train { .. }
                | Command::UnitMorph { .. }
                | Command::BuildingMorph { .. }
                | Command::Research { .. }
                | Command::Upgrade { .. }
                | Command::TrainFighter
        )
    }
}

/// Parse the command stream from decompressed section 2 data.
///
/// The stream is a sequence of blocks:
///   [u32 frame] [u8 block_size] [block_size bytes of commands]
///
/// Each block contains one or more commands:
///   [u8 player_id] [u8 command_type] [payload...]
pub fn parse_commands(data: &[u8]) -> Vec<GameCommand> {
    let mut commands = Vec::new();
    let mut pos = 0;

    while pos + 5 <= data.len() {
        let frame = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let block_size = data[pos + 4] as usize;
        pos += 5;

        if pos + block_size > data.len() {
            break;
        }

        let block_end = pos + block_size;
        let mut bpos = pos;

        while bpos + 2 <= block_end {
            let player_id = data[bpos];
            let type_id = data[bpos + 1];
            bpos += 2;

            let remaining = block_end - bpos;
            let (command, consumed) =
                parse_single_command(type_id, &data[bpos..block_end], remaining);
            bpos += consumed;

            commands.push(GameCommand {
                frame,
                player_id,
                command,
            });
        }

        pos = block_end;
    }

    commands
}

/// Parse a single command given its type_id and the remaining payload bytes.
/// Returns (Command, bytes_consumed).
fn parse_single_command(type_id: u8, data: &[u8], _remaining: usize) -> (Command, usize) {
    match type_id {
        0x05 => (Command::KeepAlive, 0),
        0x08 => (Command::Other { type_id }, 0), // Restart Game
        0x09 => parse_select(data, Command::Select { unit_tags: vec![] }),
        0x0A => parse_select(data, Command::SelectAdd { unit_tags: vec![] }),
        0x0B => parse_select(data, Command::SelectRemove { unit_tags: vec![] }),
        0x0C if data.len() >= 7 => {
            let cmd = Command::Build {
                order: data[0],
                x: u16::from_le_bytes([data[1], data[2]]),
                y: u16::from_le_bytes([data[3], data[4]]),
                unit_type: u16::from_le_bytes([data[5], data[6]]),
            };
            (cmd, 7)
        }
        0x0D => (Command::Other { type_id }, 2.min(data.len())), // Vision
        0x0E => (Command::Other { type_id }, 4.min(data.len())), // Alliance
        0x0F => (Command::Other { type_id }, 1.min(data.len())), // Game Speed
        0x10 => (Command::Other { type_id }, 0),                 // Pause
        0x11 => (Command::Other { type_id }, 0),                 // Resume
        0x12 => (Command::Other { type_id }, 4.min(data.len())), // Cheat
        0x13 if data.len() >= 2 => {
            let action = if data[0] == 0 {
                HotkeyAction::Assign
            } else {
                HotkeyAction::Select
            };
            (
                Command::Hotkey {
                    action,
                    group: data[1],
                },
                2,
            )
        }
        0x14 if data.len() >= 9 => {
            let cmd = Command::RightClick {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
                target_tag: u16::from_le_bytes([data[4], data[5]]),
                unit_type: u16::from_le_bytes([data[6], data[7]]),
                queued: data[8] != 0,
            };
            (cmd, 9)
        }
        0x15 if data.len() >= 10 => {
            let cmd = Command::TargetedOrder {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
                target_tag: u16::from_le_bytes([data[4], data[5]]),
                unit_type: u16::from_le_bytes([data[6], data[7]]),
                order: data[8],
                queued: data[9] != 0,
            };
            (cmd, 10)
        }
        0x18 => (Command::CancelBuild, 0),
        0x19 => (Command::CancelMorph, 0),
        0x1A if !data.is_empty() => (
            Command::Stop {
                queued: data[0] != 0,
            },
            1,
        ),
        0x1B => (Command::Other { type_id }, 0), // Carrier Stop
        0x1C => (Command::Other { type_id }, 0), // Reaver Stop
        0x1D => (Command::Other { type_id }, 0), // Order Nothing
        0x1E if !data.is_empty() => (
            Command::ReturnCargo {
                queued: data[0] != 0,
            },
            1,
        ),
        0x1F if data.len() >= 2 => {
            let unit_type = u16::from_le_bytes([data[0], data[1]]);
            (Command::Train { unit_type }, 2)
        }
        0x20 if data.len() >= 2 => {
            let unit_tag = u16::from_le_bytes([data[0], data[1]]);
            (Command::CancelTrain { unit_tag }, 2)
        }
        0x21 if !data.is_empty() => (
            Command::Cloak {
                queued: data[0] != 0,
            },
            1,
        ),
        0x22 if !data.is_empty() => (
            Command::Decloak {
                queued: data[0] != 0,
            },
            1,
        ),
        0x23 if data.len() >= 2 => {
            let unit_type = u16::from_le_bytes([data[0], data[1]]);
            (Command::UnitMorph { unit_type }, 2)
        }
        0x25 if !data.is_empty() => (
            Command::Unsiege {
                queued: data[0] != 0,
            },
            1,
        ),
        0x26 if !data.is_empty() => (
            Command::Siege {
                queued: data[0] != 0,
            },
            1,
        ),
        0x27 => (Command::TrainFighter, 0),
        0x28 if !data.is_empty() => (
            Command::UnloadAll {
                queued: data[0] != 0,
            },
            1,
        ),
        0x29 if data.len() >= 2 => {
            let unit_tag = u16::from_le_bytes([data[0], data[1]]);
            (Command::Unload { unit_tag }, 2)
        }
        0x2A => (Command::MergeArchon, 0),
        0x2B if !data.is_empty() => (
            Command::HoldPosition {
                queued: data[0] != 0,
            },
            1,
        ),
        0x2C if !data.is_empty() => (
            Command::Burrow {
                queued: data[0] != 0,
            },
            1,
        ),
        0x2D if !data.is_empty() => (
            Command::Unburrow {
                queued: data[0] != 0,
            },
            1,
        ),
        0x2E => (Command::CancelNuke, 0),
        0x2F if data.len() >= 4 => {
            let cmd = Command::LiftOff {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
            };
            (cmd, 4)
        }
        0x30 if !data.is_empty() => (Command::Research { tech_type: data[0] }, 1),
        0x31 => (Command::CancelResearch, 0),
        0x32 if !data.is_empty() => (
            Command::Upgrade {
                upgrade_type: data[0],
            },
            1,
        ),
        0x33 => (Command::CancelUpgrade, 0),
        0x34 => (Command::CancelAddon, 0),
        0x35 if data.len() >= 2 => {
            let unit_type = u16::from_le_bytes([data[0], data[1]]);
            (Command::BuildingMorph { unit_type }, 2)
        }
        0x36 => (Command::Stim, 0),
        0x37 => (Command::Other { type_id }, 6.min(data.len())), // Sync
        0x38 | 0x39 => (Command::Other { type_id }, 0),          // Voice enable/disable
        0x3A | 0x3B => (Command::Other { type_id }, 1.min(data.len())), // Voice squelch
        0x3C => (Command::Other { type_id }, 0),                 // Start Game
        0x3D => (Command::Other { type_id }, 1.min(data.len())), // Download %
        0x3E => (Command::Other { type_id }, 5.min(data.len())), // Change Game Slot
        0x3F => (Command::Other { type_id }, 7.min(data.len())), // New Net Player
        0x40 => (Command::Other { type_id }, 17.min(data.len())), // Joined Game
        0x41 => (Command::Other { type_id }, 2.min(data.len())), // Change Race
        0x42 | 0x43 => (Command::Other { type_id }, 1.min(data.len())), // Team
        0x44 => (Command::Other { type_id }, 2.min(data.len())), // Melee Team
        0x45 => (Command::Other { type_id }, 2.min(data.len())), // Swap Players
        0x48 => (Command::Other { type_id }, 12.min(data.len())), // Saved Data
        0x54 => (Command::Other { type_id }, 0),                 // Briefing Start
        0x55 => (Command::Other { type_id }, 1.min(data.len())), // Latency
        0x56 => (Command::Other { type_id }, 9.min(data.len())), // Replay Speed
        0x57 if !data.is_empty() => (Command::LeaveGame { reason: data[0] }, 1),
        0x58 if data.len() >= 4 => {
            let cmd = Command::MinimapPing {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
            };
            (cmd, 4)
        }
        0x5A => (Command::MergeDarkArchon, 0),
        0x5B => (Command::Other { type_id }, 0), // Make Game Public
        0x5C if data.len() >= 81 => {
            let sender_slot = data[0];
            let msg_bytes = &data[1..81];
            let end = msg_bytes.iter().position(|&b| b == 0).unwrap_or(80);
            let message = String::from_utf8_lossy(&msg_bytes[..end]).into_owned();
            (
                Command::Chat {
                    sender_slot,
                    message,
                },
                81,
            )
        }
        // 1.21+ extended commands with 4-byte unit tags.
        0x60 if data.len() >= 11 => {
            let cmd = Command::RightClick {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
                target_tag: u16::from_le_bytes([data[4], data[5]]),
                // data[6..8] = unknown 2 bytes
                unit_type: u16::from_le_bytes([data[8], data[9]]),
                queued: data[10] != 0,
            };
            (cmd, 11)
        }
        0x61 if data.len() >= 12 => {
            let cmd = Command::TargetedOrder {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
                target_tag: u16::from_le_bytes([data[4], data[5]]),
                // data[6..8] = unknown 2 bytes
                unit_type: u16::from_le_bytes([data[8], data[9]]),
                order: data[10],
                queued: data[11] != 0,
            };
            (cmd, 12)
        }
        0x62 if data.len() >= 4 => {
            let unit_tag = u16::from_le_bytes([data[0], data[1]]);
            (Command::Unload { unit_tag }, 4) // 2 extra unknown bytes
        }
        0x63 => parse_select_121(data, |tags| Command::Select { unit_tags: tags }),
        0x64 => parse_select_121(data, |tags| Command::SelectAdd { unit_tags: tags }),
        0x65 => parse_select_121(data, |tags| Command::SelectRemove { unit_tags: tags }),
        // Unknown command — consume 0 bytes and hope the block boundary saves us.
        _ => (Command::Other { type_id }, 0),
    }
}

/// Parse a pre-1.21 Select/SelectAdd/SelectRemove command.
/// Format: [u8 count] [u16 unit_tag * count]
fn parse_select(data: &[u8], _template: Command) -> (Command, usize) {
    if data.is_empty() {
        return (Command::Select { unit_tags: vec![] }, 0);
    }
    let count = data[0] as usize;
    let needed = 1 + count * 2;
    if data.len() < needed {
        return (Command::Select { unit_tags: vec![] }, data.len());
    }
    let mut tags = Vec::with_capacity(count);
    for i in 0..count {
        let off = 1 + i * 2;
        tags.push(u16::from_le_bytes([data[off], data[off + 1]]));
    }
    // Determine which variant based on template.
    let cmd = match _template {
        Command::SelectAdd { .. } => Command::SelectAdd { unit_tags: tags },
        Command::SelectRemove { .. } => Command::SelectRemove { unit_tags: tags },
        _ => Command::Select { unit_tags: tags },
    };
    (cmd, needed)
}

/// Parse a 1.21+ Select command with 4-byte unit tags.
/// Format: [u8 count] [(u16 tag + u16 unknown) * count]
fn parse_select_121(data: &[u8], make: fn(Vec<u16>) -> Command) -> (Command, usize) {
    if data.is_empty() {
        return (make(vec![]), 0);
    }
    let count = data[0] as usize;
    let needed = 1 + count * 4;
    if data.len() < needed {
        return (make(vec![]), data.len());
    }
    let mut tags = Vec::with_capacity(count);
    for i in 0..count {
        let off = 1 + i * 4;
        tags.push(u16::from_le_bytes([data[off], data[off + 1]]));
    }
    (make(tags), needed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_stream() {
        let commands = parse_commands(&[]);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_parse_single_train_command() {
        // Frame 100, block_size=4, player 0, Train(type=0x0025 = Zergling)
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_le_bytes()); // frame
        data.push(4); // block_size
        data.push(0); // player_id
        data.push(0x1F); // Train command
        data.extend_from_slice(&0x0025u16.to_le_bytes()); // unit_type

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].frame, 100);
        assert_eq!(commands[0].player_id, 0);
        assert!(matches!(
            commands[0].command,
            Command::Train { unit_type: 0x25 }
        ));
    }

    #[test]
    fn test_parse_build_command() {
        let mut data = Vec::new();
        data.extend_from_slice(&200u32.to_le_bytes());
        data.push(9); // block_size = 2 (header) + 7 (payload)
        data.push(1); // player 1
        data.push(0x0C); // Build
        data.push(0x0F); // order
        data.extend_from_slice(&100u16.to_le_bytes()); // x
        data.extend_from_slice(&200u16.to_le_bytes()); // y
        data.extend_from_slice(&0x007Du16.to_le_bytes()); // unit_type (Gateway)

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 1);
        if let Command::Build {
            order,
            x,
            y,
            unit_type,
        } = &commands[0].command
        {
            assert_eq!(*order, 0x0F);
            assert_eq!(*x, 100);
            assert_eq!(*y, 200);
            assert_eq!(*unit_type, 0x007D);
        } else {
            panic!("expected Build command");
        }
    }

    #[test]
    fn test_parse_hotkey_command() {
        let mut data = Vec::new();
        data.extend_from_slice(&50u32.to_le_bytes());
        data.push(4);
        data.push(0);
        data.push(0x13);
        data.push(0); // assign
        data.push(1); // group 1

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0].command,
            Command::Hotkey {
                action: HotkeyAction::Assign,
                group: 1
            }
        ));
    }

    #[test]
    fn test_parse_multiple_commands_in_block() {
        let mut data = Vec::new();
        data.extend_from_slice(&300u32.to_le_bytes());
        // Two commands: Train + Stop = (2+2) + (2+1) = 7 bytes
        data.push(7);
        // Command 1: player 0, Train
        data.push(0);
        data.push(0x1F);
        data.extend_from_slice(&0x0001u16.to_le_bytes());
        // Command 2: player 0, Stop
        data.push(0);
        data.push(0x1A);
        data.push(0); // not queued

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 2);
        assert!(matches!(commands[0].command, Command::Train { .. }));
        assert!(matches!(
            commands[1].command,
            Command::Stop { queued: false }
        ));
    }

    #[test]
    fn test_meaningful_and_effective_actions() {
        assert!(!Command::KeepAlive.is_meaningful_action());
        assert!(!Command::Select { unit_tags: vec![1] }.is_meaningful_action());
        assert!(Command::Train { unit_type: 1 }.is_meaningful_action());
        assert!(
            Command::Build {
                order: 0,
                x: 0,
                y: 0,
                unit_type: 1
            }
            .is_meaningful_action()
        );

        // Hotkey assign is effective, select is not
        assert!(
            Command::Hotkey {
                action: HotkeyAction::Assign,
                group: 1
            }
            .is_effective_action()
        );
        assert!(
            !Command::Hotkey {
                action: HotkeyAction::Select,
                group: 1
            }
            .is_effective_action()
        );
    }

    #[test]
    fn test_build_order_actions() {
        assert!(
            Command::Build {
                order: 0,
                x: 0,
                y: 0,
                unit_type: 1
            }
            .is_build_order_action()
        );
        assert!(Command::Train { unit_type: 1 }.is_build_order_action());
        assert!(Command::Research { tech_type: 1 }.is_build_order_action());
        assert!(Command::Upgrade { upgrade_type: 1 }.is_build_order_action());
        assert!(Command::UnitMorph { unit_type: 1 }.is_build_order_action());
        assert!(!Command::Stop { queued: false }.is_build_order_action());
        assert!(!Command::KeepAlive.is_build_order_action());
    }
}
