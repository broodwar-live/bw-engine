use crate::fp8::Fp8;

/// 256-direction type matching OpenBW's `direction_t`.
/// 0 = north, 64 = east, 128 = south, 192 = west.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Direction(pub u8);

impl Direction {
    pub const NORTH: Self = Self(0);
    pub const EAST: Self = Self(64);
    pub const SOUTH: Self = Self(128);
    pub const WEST: Self = Self(192);

    /// Compute direction from a position delta.
    /// Finds the closest match in the direction_table (brute force over 256 entries).
    /// This matches OpenBW's atan2-based approach for accuracy.
    #[must_use]
    pub fn from_delta(dx: Fp8, dy: Fp8) -> Self {
        if dx.0 == 0 && dy.0 == 0 {
            return Self(0);
        }

        // Find the direction whose unit vector is most aligned with (dx, dy).
        // Maximize dot product; on ties, minimize |cross product| (angular distance).
        let mut best_dir: u8 = 0;
        let mut best_dot: i64 = i64::MIN;
        let mut best_cross: i64 = i64::MAX;

        let dxi = dx.0 as i64;
        let dyi = dy.0 as i64;

        for i in 0u16..256 {
            let tx = DIRECTION_TABLE_X[i as usize] as i64;
            let ty = DIRECTION_TABLE_Y[i as usize] as i64;
            let dot = dxi * tx + dyi * ty;
            let cross = (dxi * ty - dyi * tx).abs();
            if dot > best_dot || (dot == best_dot && cross < best_cross) {
                best_dot = dot;
                best_cross = cross;
                best_dir = i as u8;
            }
        }

        Self(best_dir)
    }

    /// Turn toward `target` by at most `rate` direction units, choosing the shorter arc.
    #[must_use]
    pub fn turn_toward(self, target: Self, rate: u8) -> Self {
        let diff = target.0.wrapping_sub(self.0) as i8;
        if diff == 0 {
            return self;
        }
        let clamped = if diff > 0 {
            diff.min(rate as i8)
        } else {
            diff.max(-(rate as i8))
        };
        Self(self.0.wrapping_add(clamped as u8))
    }

    /// Signed shortest angular difference from self to other.
    #[must_use]
    pub fn diff(self, other: Self) -> i8 {
        other.0.wrapping_sub(self.0) as i8
    }

    /// Get the unit velocity vector for this direction (from OpenBW's direction_table).
    /// Returns (dx, dy) in fp8 where the magnitude is 256 (= 1.0 in fp8).
    #[must_use]
    pub fn unit_vector(self) -> (Fp8, Fp8) {
        let idx = self.0 as usize;
        (
            Fp8::from_raw(DIRECTION_TABLE_X[idx] as i32),
            Fp8::from_raw(DIRECTION_TABLE_Y[idx] as i32),
        )
    }
}

/// X components of the direction unit vectors, from OpenBW's direction_table.
/// Index = direction (0-255), value = x component in fp8 (i16 range).
const DIRECTION_TABLE_X: [i16; 256] = [
    0, 6, 13, 19, 25, 31, 38, 44,
    50, 56, 62, 68, 74, 80, 86, 92,
    98, 104, 109, 115, 121, 126, 132, 137,
    142, 147, 152, 157, 162, 167, 172, 177,
    181, 185, 190, 194, 198, 202, 206, 209,
    213, 216, 220, 223, 226, 229, 231, 234,
    237, 239, 241, 243, 245, 247, 248, 250,
    251, 252, 253, 254, 255, 255, 256, 256,
    256, 256, 256, 255, 255, 254, 253, 252,
    251, 250, 248, 247, 245, 243, 241, 239,
    237, 234, 231, 229, 226, 223, 220, 216,
    213, 209, 206, 202, 198, 194, 190, 185,
    181, 177, 172, 167, 162, 157, 152, 147,
    142, 137, 132, 126, 121, 115, 109, 104,
    98, 92, 86, 80, 74, 68, 62, 56,
    50, 44, 38, 31, 25, 19, 13, 6,
    0, -6, -13, -19, -25, -31, -38, -44,
    -50, -56, -62, -68, -74, -80, -86, -92,
    -98, -104, -109, -115, -121, -126, -132, -137,
    -142, -147, -152, -157, -162, -167, -172, -177,
    -181, -185, -190, -194, -198, -202, -206, -209,
    -213, -216, -220, -223, -226, -229, -231, -234,
    -237, -239, -241, -243, -245, -247, -248, -250,
    -251, -252, -253, -254, -255, -255, -256, -256,
    -256, -256, -256, -255, -255, -254, -253, -252,
    -251, -250, -248, -247, -245, -243, -241, -239,
    -237, -234, -231, -229, -226, -223, -220, -216,
    -213, -209, -206, -202, -198, -194, -190, -185,
    -181, -177, -172, -167, -162, -157, -152, -147,
    -142, -137, -132, -126, -121, -115, -109, -104,
    -98, -92, -86, -80, -74, -68, -62, -56,
    -50, -44, -38, -31, -25, -19, -13, -6,
];

/// Y components of the direction unit vectors.
const DIRECTION_TABLE_Y: [i16; 256] = [
    -256, -256, -256, -255, -255, -254, -253, -252,
    -251, -250, -248, -247, -245, -243, -241, -239,
    -237, -234, -231, -229, -226, -223, -220, -216,
    -213, -209, -206, -202, -198, -194, -190, -185,
    -181, -177, -172, -167, -162, -157, -152, -147,
    -142, -137, -132, -126, -121, -115, -109, -104,
    -98, -92, -86, -80, -74, -68, -62, -56,
    -50, -44, -38, -31, -25, -19, -13, -6,
    0, 6, 13, 19, 25, 31, 38, 44,
    50, 56, 62, 68, 74, 80, 86, 92,
    98, 104, 109, 115, 121, 126, 132, 137,
    142, 147, 152, 157, 162, 167, 172, 177,
    181, 185, 190, 194, 198, 202, 206, 209,
    213, 216, 220, 223, 226, 229, 231, 234,
    237, 239, 241, 243, 245, 247, 248, 250,
    251, 252, 253, 254, 255, 255, 256, 256,
    256, 256, 256, 255, 255, 254, 253, 252,
    251, 250, 248, 247, 245, 243, 241, 239,
    237, 234, 231, 229, 226, 223, 220, 216,
    213, 209, 206, 202, 198, 194, 190, 185,
    181, 177, 172, 167, 162, 157, 152, 147,
    142, 137, 132, 126, 121, 115, 109, 104,
    98, 92, 86, 80, 74, 68, 62, 56,
    50, 44, 38, 31, 25, 19, 13, 6,
    0, -6, -13, -19, -25, -31, -38, -44,
    -50, -56, -62, -68, -74, -80, -86, -92,
    -98, -104, -109, -115, -121, -126, -132, -137,
    -142, -147, -152, -157, -162, -167, -172, -177,
    -181, -185, -190, -194, -198, -202, -206, -209,
    -213, -216, -220, -223, -226, -229, -231, -234,
    -237, -239, -241, -243, -245, -247, -248, -250,
    -251, -252, -253, -254, -255, -255, -256, -256,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardinal_directions() {
        // North: positive Y is down in screen coords, so dy<0 = north
        let north = Direction::from_delta(Fp8::from_pixels(0), Fp8::from_pixels(-10));
        assert_eq!(north.0, 0);

        // East: dx>0, dy=0
        let east = Direction::from_delta(Fp8::from_pixels(10), Fp8::from_pixels(0));
        assert_eq!(east.0, 64);

        // South: dy>0
        let south = Direction::from_delta(Fp8::from_pixels(0), Fp8::from_pixels(10));
        assert_eq!(south.0, 128);

        // West: dx<0
        let west = Direction::from_delta(Fp8::from_pixels(-10), Fp8::from_pixels(0));
        assert_eq!(west.0, 192);
    }

    #[test]
    fn test_diagonal_directions() {
        // NE: dx>0, dy<0, equal magnitude -> direction 32
        let ne = Direction::from_delta(Fp8::from_pixels(10), Fp8::from_pixels(-10));
        assert_eq!(ne.0, 32);

        // SE: dx>0, dy>0, equal magnitude -> direction 96
        let se = Direction::from_delta(Fp8::from_pixels(10), Fp8::from_pixels(10));
        assert_eq!(se.0, 96);
    }

    #[test]
    fn test_zero_delta() {
        assert_eq!(Direction::from_delta(Fp8::ZERO, Fp8::ZERO).0, 0);
    }

    #[test]
    fn test_turn_toward_short_arc() {
        let a = Direction(10);
        let b = Direction(20);
        // Turn 5 units toward b -> should be 15
        assert_eq!(a.turn_toward(b, 5).0, 15);
    }

    #[test]
    fn test_turn_toward_arrives() {
        let a = Direction(10);
        let b = Direction(13);
        // Turn 5 units, but only 3 away -> should arrive at target
        assert_eq!(a.turn_toward(b, 5).0, 13);
    }

    #[test]
    fn test_turn_toward_negative() {
        let a = Direction(20);
        let b = Direction(10);
        // Turn backward
        assert_eq!(a.turn_toward(b, 5).0, 15);
    }

    #[test]
    fn test_turn_toward_wrapping() {
        let a = Direction(250);
        let b = Direction(5);
        // Shortest arc: 250 -> 255 -> 0 -> 5 = 11 steps forward
        // Turn 5 units -> 255
        assert_eq!(a.turn_toward(b, 5).0, 255);
    }

    #[test]
    fn test_turn_toward_same() {
        let a = Direction(42);
        assert_eq!(a.turn_toward(a, 10).0, 42);
    }

    #[test]
    fn test_diff() {
        assert_eq!(Direction(10).diff(Direction(20)), 10);
        assert_eq!(Direction(20).diff(Direction(10)), -10);
        // Wrapping: 250 to 5 = +11
        assert_eq!(Direction(250).diff(Direction(5)), 11);
    }

    #[test]
    fn test_unit_vector_north() {
        let (dx, dy) = Direction::NORTH.unit_vector();
        assert_eq!(dx.raw(), 0);
        assert_eq!(dy.raw(), -256);
    }

    #[test]
    fn test_unit_vector_east() {
        let (dx, dy) = Direction::EAST.unit_vector();
        assert_eq!(dx.raw(), 256);
        assert_eq!(dy.raw(), 0);
    }

    #[test]
    fn test_unit_vector_south() {
        let (dx, dy) = Direction::SOUTH.unit_vector();
        assert_eq!(dx.raw(), 0);
        assert_eq!(dy.raw(), 256);
    }

    #[test]
    fn test_unit_vector_west() {
        let (dx, dy) = Direction::WEST.unit_vector();
        assert_eq!(dx.raw(), -256);
        assert_eq!(dy.raw(), 0);
    }
}
