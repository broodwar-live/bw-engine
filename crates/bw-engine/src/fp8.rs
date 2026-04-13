/// Fixed-point 24.8 arithmetic matching OpenBW's `fp8` type.
/// Lower 8 bits are fractional, upper 24 bits are integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fp8(pub i32);

impl Fp8 {
    pub const ZERO: Self = Self(0);

    /// Create from an integer pixel value (shifts left 8).
    #[must_use]
    pub fn from_pixels(px: i32) -> Self {
        Self(px << 8)
    }

    /// Extract integer pixel value (shifts right 8).
    #[must_use]
    pub fn to_pixels(self) -> i32 {
        self.0 >> 8
    }

    /// Construct from raw fp8 representation.
    #[must_use]
    pub fn from_raw(v: i32) -> Self {
        Self(v)
    }

    /// Access the raw i32 value.
    #[must_use]
    pub fn raw(self) -> i32 {
        self.0
    }

    /// Absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl std::ops::Add for Fp8 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl std::ops::AddAssign for Fp8 {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_add(rhs.0);
    }
}

impl std::ops::Sub for Fp8 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl std::ops::SubAssign for Fp8 {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_sub(rhs.0);
    }
}

impl std::ops::Neg for Fp8 {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl std::ops::Mul<i32> for Fp8 {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self(self.0 * rhs)
    }
}

impl std::ops::Div<i32> for Fp8 {
    type Output = Self;
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
    }
}

/// 2D position in fp8 coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct XY {
    pub x: Fp8,
    pub y: Fp8,
}

impl XY {
    pub const ZERO: Self = Self {
        x: Fp8::ZERO,
        y: Fp8::ZERO,
    };

    /// Create from integer pixel coordinates.
    #[must_use]
    pub fn from_pixels(x: i32, y: i32) -> Self {
        Self {
            x: Fp8::from_pixels(x),
            y: Fp8::from_pixels(y),
        }
    }

    /// Convert to integer pixel coordinates.
    #[must_use]
    pub fn to_pixels(self) -> (i32, i32) {
        (self.x.to_pixels(), self.y.to_pixels())
    }

    /// Squared length (avoids sqrt). Returns i64 to avoid overflow.
    #[must_use]
    pub fn length_squared(self) -> i64 {
        let x = self.x.0 as i64;
        let y = self.y.0 as i64;
        x * x + y * y
    }
}

impl std::ops::Add for XY {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl std::ops::AddAssign for XY {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl std::ops::Sub for XY {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

/// Integer square root (Newton's method).
pub fn isqrt(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_to_pixels() {
        assert_eq!(Fp8::from_pixels(100).to_pixels(), 100);
        assert_eq!(Fp8::from_pixels(-50).to_pixels(), -50);
        assert_eq!(Fp8::from_pixels(0).to_pixels(), 0);
    }

    #[test]
    fn test_from_raw() {
        let fp = Fp8::from_raw(256);
        assert_eq!(fp.to_pixels(), 1);
        let fp = Fp8::from_raw(128);
        assert_eq!(fp.to_pixels(), 0); // 0.5 truncates to 0
    }

    #[test]
    fn test_arithmetic() {
        let a = Fp8::from_pixels(10);
        let b = Fp8::from_pixels(3);
        assert_eq!((a + b).to_pixels(), 13);
        assert_eq!((a - b).to_pixels(), 7);
        assert_eq!((-a).to_pixels(), -10);
    }

    #[test]
    fn test_mul_div() {
        let a = Fp8::from_pixels(10);
        assert_eq!((a * 3).to_pixels(), 30);
        assert_eq!((a / 2).to_pixels(), 5);
    }

    #[test]
    fn test_abs() {
        assert_eq!(Fp8::from_pixels(-5).abs(), Fp8::from_pixels(5));
        assert_eq!(Fp8::from_pixels(5).abs(), Fp8::from_pixels(5));
    }

    #[test]
    fn test_xy_from_to_pixels() {
        let pos = XY::from_pixels(100, 200);
        assert_eq!(pos.to_pixels(), (100, 200));
    }

    #[test]
    fn test_xy_add_sub() {
        let a = XY::from_pixels(10, 20);
        let b = XY::from_pixels(3, 7);
        assert_eq!((a + b).to_pixels(), (13, 27));
        assert_eq!((a - b).to_pixels(), (7, 13));
    }

    #[test]
    fn test_xy_length_squared() {
        let v = XY::from_pixels(3, 4);
        // (3*256)^2 + (4*256)^2 = 768^2 + 1024^2 = 589824 + 1048576 = 1638400
        assert_eq!(v.length_squared(), 1_638_400);
    }

    #[test]
    fn test_isqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(10), 3);
        assert_eq!(isqrt(100), 10);
    }

    #[test]
    fn test_default() {
        assert_eq!(Fp8::default(), Fp8::ZERO);
        assert_eq!(XY::default(), XY::ZERO);
    }
}
