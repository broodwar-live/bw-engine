//! SCX/SCM map file loader for StarCraft: Brood War.
//!
//! SCX (Brood War) and SCM (vanilla StarCraft) map files are MPQ archives
//! containing a `staredit\scenario.chk` file with the map data.
//!
//! This module provides a convenience API to open a map file, extract the CHK,
//! and optionally load tileset data from a game data MPQ archive.

use crate::chk;
use crate::chk_units::{self, ChkUnit};
use crate::error::Result;
use crate::mpq::MpqArchive;

/// The path to the scenario CHK inside a map MPQ.
const SCENARIO_PATH: &str = "staredit\\scenario.chk";

/// A loaded SCX/SCM map file.
pub struct ScxMap {
    /// Raw CHK data extracted from the map MPQ.
    pub chk_data: Vec<u8>,
    /// Parsed CHK terrain data.
    pub terrain: chk::ChkTerrain,
    /// Parsed CHK unit placements.
    pub units: Vec<ChkUnit>,
}

impl ScxMap {
    /// Open a map file from raw `.scx` or `.scm` bytes.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let archive = MpqArchive::from_bytes(data)?;

        let chk_data = archive.read_file(SCENARIO_PATH)?;

        let sections = chk::parse_sections(&chk_data)?;
        let terrain = chk::extract_terrain(&sections)?;
        let units = chk_units::parse_chk_units(&sections)?;

        Ok(Self {
            chk_data,
            terrain,
            units,
        })
    }

    /// Get the map dimensions in tiles.
    pub fn dimensions(&self) -> (u16, u16) {
        (self.terrain.width, self.terrain.height)
    }

    /// Get the tileset index (0-7).
    pub fn tileset_index(&self) -> u16 {
        self.terrain.tileset_index
    }

    /// Get the tileset.
    pub fn tileset(&self) -> Result<crate::tileset::Tileset> {
        crate::tileset::Tileset::from_index(self.terrain.tileset_index)
    }

    /// Build a `Map` from this SCX file's CHK data + external tileset files.
    ///
    /// `cv5_data` and `vf4_data` are the tileset files for the map's tileset.
    pub fn to_map(&self, cv5_data: &[u8], vf4_data: &[u8]) -> Result<crate::map::Map> {
        crate::map::Map::from_chk(&self.chk_data, cv5_data, vf4_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // We can't easily build a full SCX file in tests without an MPQ writer,
    // but we can test error handling for invalid data.

    #[test]
    fn test_scx_from_invalid_data() {
        let result = ScxMap::from_bytes(vec![0u8; 64]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scx_from_empty() {
        let result = ScxMap::from_bytes(vec![]);
        assert!(result.is_err());
    }
}
