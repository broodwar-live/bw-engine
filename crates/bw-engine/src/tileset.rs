use crate::error::{EngineError, Result};

/// The 8 StarCraft tilesets, indexed 0-7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Tileset {
    Badlands = 0,
    SpacePlatform = 1,
    Installation = 2,
    Ashworld = 3,
    Jungle = 4,
    Desert = 5,
    Arctic = 6,
    Twilight = 7,
}

impl Tileset {
    pub fn from_index(index: u16) -> Result<Self> {
        match index % 8 {
            0 => Ok(Self::Badlands),
            1 => Ok(Self::SpacePlatform),
            2 => Ok(Self::Installation),
            3 => Ok(Self::Ashworld),
            4 => Ok(Self::Jungle),
            5 => Ok(Self::Desert),
            6 => Ok(Self::Arctic),
            7 => Ok(Self::Twilight),
            _ => unreachable!(),
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Badlands => "Badlands",
            Self::SpacePlatform => "Space Platform",
            Self::Installation => "Installation",
            Self::Ashworld => "Ashworld",
            Self::Jungle => "Jungle",
            Self::Desert => "Desert",
            Self::Arctic => "Arctic",
            Self::Twilight => "Twilight",
        }
    }

    #[must_use]
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::Badlands => "badlands",
            Self::SpacePlatform => "platform",
            Self::Installation => "install",
            Self::Ashworld => "AshWorld",
            Self::Jungle => "Jungle",
            Self::Desert => "Desert",
            Self::Arctic => "Ice",
            Self::Twilight => "Twilight",
        }
    }
}

/// CV5 entry size in bytes (confirmed from OpenBW bwgame.h:21117).
pub const CV5_ENTRY_SIZE: usize = 52;

/// VF4 entry size in bytes (16 x u16 mini-tile flags).
pub const VF4_ENTRY_SIZE: usize = 32;

/// Parsed CV5 entry (one per tile group).
#[derive(Debug, Clone)]
pub(crate) struct Cv5Entry {
    pub flags: u16,
    pub mega_tile_indices: [u16; 16],
}

/// Parsed VF4 entry: 16 mini-tile flags for a 4x4 grid of 8x8px mini-tiles.
#[derive(Debug, Clone)]
pub(crate) struct Vf4Entry {
    pub mini_tile_flags: [u16; 16],
}

/// Parsed tileset data ready for tile lookups.
pub struct TilesetData {
    pub(crate) cv5: Vec<Cv5Entry>,
    pub(crate) vf4: Vec<Vf4Entry>,
}

impl TilesetData {
    /// Parse from raw CV5 and VF4 file bytes.
    pub fn from_bytes(cv5_data: &[u8], vf4_data: &[u8]) -> Result<Self> {
        let cv5 = parse_cv5(cv5_data)?;
        let vf4 = parse_vf4(vf4_data)?;
        Ok(Self { cv5, vf4 })
    }

    /// Look up the VF4 megatile index for a given raw MTXM tile_id.
    ///
    /// Returns `None` if the group_index is out of bounds (treated as empty tile).
    pub(crate) fn megatile_index(&self, tile_id: u16) -> Option<u16> {
        let group_index = ((tile_id >> 4) & 0x7FF) as usize;
        let subtile_index = (tile_id & 0xF) as usize;

        let entry = self.cv5.get(group_index)?;
        Some(entry.mega_tile_indices[subtile_index])
    }

    /// Get the 4x4 mini-tile flags for a megatile.
    pub(crate) fn mini_tile_flags(&self, megatile_idx: u16) -> Result<&[u16; 16]> {
        let idx = megatile_idx as usize;
        self.vf4.get(idx).map(|e| &e.mini_tile_flags).ok_or(
            EngineError::MegatileLookupOutOfBounds {
                index: idx,
                vf4_len: self.vf4.len(),
            },
        )
    }

    /// Get the CV5 flags for a tile group.
    pub(crate) fn cv5_flags(&self, tile_id: u16) -> u16 {
        let group_index = ((tile_id >> 4) & 0x7FF) as usize;
        self.cv5.get(group_index).map(|e| e.flags).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// VX4 — megatile → mini-tile graphic references (32 bytes per entry)
// ---------------------------------------------------------------------------

/// VX4 entry size in bytes (16 x u16 mini-tile image references).
pub const VX4_ENTRY_SIZE: usize = 32;

/// A VX4 entry: 16 references to VR4 mini-tile images for a 4x4 grid.
/// Each reference is a u16 where bits 0-14 are the VR4 index and bit 15
/// indicates horizontal flip.
#[derive(Debug, Clone)]
pub struct Vx4Entry {
    /// Raw mini-tile references (VR4 index + flip bit).
    pub refs: [u16; 16],
}

impl Vx4Entry {
    /// Get the VR4 image index for a mini-tile (0-15).
    #[must_use]
    pub fn vr4_index(&self, mini_tile: usize) -> u16 {
        self.refs[mini_tile] >> 1
    }

    /// Whether a mini-tile should be horizontally flipped.
    #[must_use]
    pub fn is_flipped(&self, mini_tile: usize) -> bool {
        self.refs[mini_tile] & 1 != 0
    }
}

/// Parsed VX4 data.
pub struct Vx4Data {
    entries: Vec<Vx4Entry>,
}

impl Vx4Data {
    /// Parse from raw VX4 file bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if !data.len().is_multiple_of(VX4_ENTRY_SIZE) {
            return Err(EngineError::TilesetDataTooShort {
                file: "vx4",
                expected: VX4_ENTRY_SIZE,
                actual: data.len() % VX4_ENTRY_SIZE,
            });
        }

        let count = data.len() / VX4_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let base = i * VX4_ENTRY_SIZE;
            let mut refs = [0u16; 16];
            for (j, slot) in refs.iter_mut().enumerate() {
                *slot = read_u16_le(data, base + j * 2);
            }
            entries.push(Vx4Entry { refs });
        }

        Ok(Self { entries })
    }

    /// Get a VX4 entry by megatile index.
    pub fn get(&self, index: usize) -> Option<&Vx4Entry> {
        self.entries.get(index)
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// VR4 — 8x8 mini-tile pixel data (64 bytes per entry)
// ---------------------------------------------------------------------------

/// VR4 entry size in bytes (8x8 = 64 palette indices).
pub const VR4_ENTRY_SIZE: usize = 64;

/// A single 8x8 mini-tile: 64 palette indices, row-major.
#[derive(Debug, Clone)]
pub struct Vr4Entry {
    /// 64 palette indices (8 rows of 8 pixels).
    pub pixels: [u8; 64],
}

impl Vr4Entry {
    /// Get the pixel at (x, y) where 0 <= x,y < 8.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> u8 {
        self.pixels[y * 8 + x]
    }

    /// Get a row of 8 pixels.
    #[must_use]
    pub fn row(&self, y: usize) -> &[u8] {
        &self.pixels[y * 8..(y + 1) * 8]
    }
}

/// Parsed VR4 data.
pub struct Vr4Data {
    entries: Vec<Vr4Entry>,
}

impl Vr4Data {
    /// Parse from raw VR4 file bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if !data.len().is_multiple_of(VR4_ENTRY_SIZE) {
            return Err(EngineError::TilesetDataTooShort {
                file: "vr4",
                expected: VR4_ENTRY_SIZE,
                actual: data.len() % VR4_ENTRY_SIZE,
            });
        }

        let count = data.len() / VR4_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let base = i * VR4_ENTRY_SIZE;
            let mut pixels = [0u8; 64];
            pixels.copy_from_slice(&data[base..base + 64]);
            entries.push(Vr4Entry { pixels });
        }

        Ok(Self { entries })
    }

    /// Get a VR4 entry by index.
    pub fn get(&self, index: usize) -> Option<&Vr4Entry> {
        self.entries.get(index)
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// WPE — 256-color palette (1024 bytes: 256 x RGBX)
// ---------------------------------------------------------------------------

/// WPE file size (256 colors x 4 bytes each).
pub const WPE_SIZE: usize = 1024;

/// A single palette color.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaletteColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A 256-color palette parsed from a WPE file.
#[derive(Debug, Clone)]
pub struct Palette {
    pub colors: [PaletteColor; 256],
}

impl Palette {
    /// Parse from raw WPE file bytes (1024 bytes: 256 x RGBX).
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < WPE_SIZE {
            return Err(EngineError::TilesetDataTooShort {
                file: "wpe",
                expected: WPE_SIZE,
                actual: data.len(),
            });
        }

        let mut colors = [PaletteColor::default(); 256];
        for (i, color) in colors.iter_mut().enumerate() {
            let base = i * 4;
            color.r = data[base];
            color.g = data[base + 1];
            color.b = data[base + 2];
            // data[base + 3] is padding (unused).
        }

        Ok(Self { colors })
    }

    /// Look up a color by palette index.
    #[must_use]
    pub fn color(&self, index: u8) -> PaletteColor {
        self.colors[index as usize]
    }

    /// Convert a palette index to an RGBA u32 (0xRRGGBBAA, alpha=255).
    #[must_use]
    pub fn to_rgba(&self, index: u8) -> u32 {
        let c = self.colors[index as usize];
        u32::from_be_bytes([c.r, c.g, c.b, 0xFF])
    }
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn parse_cv5(data: &[u8]) -> Result<Vec<Cv5Entry>> {
    if !data.len().is_multiple_of(CV5_ENTRY_SIZE) {
        return Err(EngineError::TilesetDataTooShort {
            file: "cv5",
            expected: CV5_ENTRY_SIZE,
            actual: data.len() % CV5_ENTRY_SIZE,
        });
    }

    let count = data.len() / CV5_ENTRY_SIZE;
    let mut entries = Vec::with_capacity(count);

    for i in 0..count {
        let base = i * CV5_ENTRY_SIZE;
        // bytes 0-1: skipped (index/type)
        // bytes 2-3: flags
        let flags = read_u16_le(data, base + 2);
        // bytes 4-19: skipped (4x u16 misc fields)
        // bytes 20-51: 16x u16 mega_tile_indices
        let mut mega_tile_indices = [0u16; 16];
        for (j, slot) in mega_tile_indices.iter_mut().enumerate() {
            *slot = read_u16_le(data, base + 20 + j * 2);
        }
        entries.push(Cv5Entry {
            flags,
            mega_tile_indices,
        });
    }

    Ok(entries)
}

fn parse_vf4(data: &[u8]) -> Result<Vec<Vf4Entry>> {
    if !data.len().is_multiple_of(VF4_ENTRY_SIZE) {
        return Err(EngineError::TilesetDataTooShort {
            file: "vf4",
            expected: VF4_ENTRY_SIZE,
            actual: data.len() % VF4_ENTRY_SIZE,
        });
    }

    let count = data.len() / VF4_ENTRY_SIZE;
    let mut entries = Vec::with_capacity(count);

    for i in 0..count {
        let base = i * VF4_ENTRY_SIZE;
        let mut mini_tile_flags = [0u16; 16];
        for (j, slot) in mini_tile_flags.iter_mut().enumerate() {
            *slot = read_u16_le(data, base + j * 2);
        }
        entries.push(Vf4Entry { mini_tile_flags });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tileset_from_index() {
        assert_eq!(Tileset::from_index(0).unwrap(), Tileset::Badlands);
        assert_eq!(Tileset::from_index(4).unwrap(), Tileset::Jungle);
        assert_eq!(Tileset::from_index(7).unwrap(), Tileset::Twilight);
    }

    #[test]
    fn test_tileset_modulo_8() {
        assert_eq!(Tileset::from_index(8).unwrap(), Tileset::Badlands);
        assert_eq!(Tileset::from_index(12).unwrap(), Tileset::Jungle);
        assert_eq!(Tileset::from_index(255).unwrap(), Tileset::Twilight);
    }

    #[test]
    fn test_tileset_file_stems() {
        assert_eq!(Tileset::Badlands.file_stem(), "badlands");
        assert_eq!(Tileset::Arctic.file_stem(), "Ice");
        assert_eq!(Tileset::SpacePlatform.file_stem(), "platform");
    }

    fn build_cv5_entry(flags: u16, mega_tile_indices: &[u16; 16]) -> Vec<u8> {
        let mut entry = vec![0u8; CV5_ENTRY_SIZE];
        // bytes 2-3: flags
        entry[2..4].copy_from_slice(&flags.to_le_bytes());
        // bytes 20-51: mega_tile_indices
        for (j, &idx) in mega_tile_indices.iter().enumerate() {
            entry[20 + j * 2..22 + j * 2].copy_from_slice(&idx.to_le_bytes());
        }
        entry
    }

    fn build_vf4_entry(mini_tile_flags: &[u16; 16]) -> Vec<u8> {
        let mut entry = vec![0u8; VF4_ENTRY_SIZE];
        for (j, &f) in mini_tile_flags.iter().enumerate() {
            entry[j * 2..j * 2 + 2].copy_from_slice(&f.to_le_bytes());
        }
        entry
    }

    #[test]
    fn test_parse_cv5_single_entry() {
        let indices: [u16; 16] = [
            10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        ];
        let data = build_cv5_entry(0x00FF, &indices);
        let entries = parse_cv5(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].flags, 0x00FF);
        assert_eq!(entries[0].mega_tile_indices, indices);
    }

    #[test]
    fn test_parse_vf4_single_entry() {
        let flags: [u16; 16] = [1, 0, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 0, 1, 1];
        let data = build_vf4_entry(&flags);
        let entries = parse_vf4(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].mini_tile_flags, flags);
    }

    #[test]
    fn test_parse_cv5_bad_size() {
        let data = vec![0u8; 51]; // not a multiple of 52
        assert!(parse_cv5(&data).is_err());
    }

    #[test]
    fn test_parse_vf4_bad_size() {
        let data = vec![0u8; 33]; // not a multiple of 32
        assert!(parse_vf4(&data).is_err());
    }

    #[test]
    fn test_megatile_index_lookup() {
        let mut indices = [0u16; 16];
        indices[5] = 42; // subtile 5 -> megatile 42
        let cv5_data = build_cv5_entry(0, &indices);
        let vf4_data = build_vf4_entry(&[0u16; 16]);

        let ts = TilesetData::from_bytes(&cv5_data, &vf4_data).unwrap();

        // tile_id with group_index=0, subtile=5: (0 << 4) | 5 = 5
        assert_eq!(ts.megatile_index(0x0005), Some(42));
    }

    #[test]
    fn test_megatile_index_out_of_bounds() {
        let cv5_data = build_cv5_entry(0, &[0u16; 16]); // 1 entry = group 0 only
        let vf4_data = build_vf4_entry(&[0u16; 16]);
        let ts = TilesetData::from_bytes(&cv5_data, &vf4_data).unwrap();

        // group_index=1 is out of bounds -> None
        let tile_id = 1u16 << 4;
        assert_eq!(ts.megatile_index(tile_id), None);
    }

    #[test]
    fn test_tile_id_encoding() {
        // group_index=3, subtile=7 -> raw = (3 << 4) | 7 = 55
        let raw: u16 = (3 << 4) | 7;
        assert_eq!((raw >> 4) & 0x7FF, 3);
        assert_eq!(raw & 0xF, 7);
    }

    // -- VX4 tests --

    #[test]
    fn test_parse_vx4() {
        let mut data = vec![0u8; VX4_ENTRY_SIZE];
        // Mini-tile 0: VR4 index 42, not flipped → (42 << 1) | 0 = 84
        data[0..2].copy_from_slice(&84u16.to_le_bytes());
        // Mini-tile 1: VR4 index 10, flipped → (10 << 1) | 1 = 21
        data[2..4].copy_from_slice(&21u16.to_le_bytes());

        let vx4 = Vx4Data::from_bytes(&data).unwrap();
        assert_eq!(vx4.len(), 1);
        let entry = vx4.get(0).unwrap();
        assert_eq!(entry.vr4_index(0), 42);
        assert!(!entry.is_flipped(0));
        assert_eq!(entry.vr4_index(1), 10);
        assert!(entry.is_flipped(1));
    }

    #[test]
    fn test_parse_vx4_bad_size() {
        assert!(Vx4Data::from_bytes(&[0u8; 31]).is_err());
    }

    // -- VR4 tests --

    #[test]
    fn test_parse_vr4() {
        let mut data = vec![0u8; VR4_ENTRY_SIZE];
        // Set pixel (3, 2) = palette index 42.
        data[2 * 8 + 3] = 42;

        let vr4 = Vr4Data::from_bytes(&data).unwrap();
        assert_eq!(vr4.len(), 1);
        let entry = vr4.get(0).unwrap();
        assert_eq!(entry.pixel(3, 2), 42);
        assert_eq!(entry.pixel(0, 0), 0);
    }

    #[test]
    fn test_vr4_row() {
        let mut data = vec![0u8; VR4_ENTRY_SIZE];
        for i in 0..8 {
            data[3 * 8 + i] = i as u8 + 1; // row 3: [1,2,3,4,5,6,7,8]
        }
        let vr4 = Vr4Data::from_bytes(&data).unwrap();
        assert_eq!(vr4.get(0).unwrap().row(3), &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn test_parse_vr4_bad_size() {
        assert!(Vr4Data::from_bytes(&[0u8; 63]).is_err());
    }

    // -- WPE tests --

    #[test]
    fn test_parse_wpe() {
        let mut data = vec![0u8; WPE_SIZE];
        // Color 0: red (255, 0, 0, padding)
        data[0] = 255;
        // Color 1: green (0, 255, 0, padding)
        data[5] = 255;
        // Color 255: blue (0, 0, 255, padding)
        data[255 * 4 + 2] = 255;

        let palette = Palette::from_bytes(&data).unwrap();
        assert_eq!(palette.color(0), PaletteColor { r: 255, g: 0, b: 0 });
        assert_eq!(palette.color(1), PaletteColor { r: 0, g: 255, b: 0 });
        assert_eq!(palette.color(255), PaletteColor { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn test_wpe_to_rgba() {
        let mut data = vec![0u8; WPE_SIZE];
        data[0] = 0xAA;
        data[1] = 0xBB;
        data[2] = 0xCC;
        let palette = Palette::from_bytes(&data).unwrap();
        assert_eq!(palette.to_rgba(0), 0xAABBCCFF);
    }

    #[test]
    fn test_parse_wpe_too_short() {
        assert!(Palette::from_bytes(&[0u8; 100]).is_err());
    }
}
