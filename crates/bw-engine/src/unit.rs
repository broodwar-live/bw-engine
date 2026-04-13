use crate::direction::Direction;
use crate::fp8::{Fp8, XY};

/// Unit ID encoding: bits 0-10 = index (0-1699), bits 11-15 = generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitId {
    index: u16,
    generation: u16,
}

impl UnitId {
    pub fn new(index: u16, generation: u16) -> Self {
        Self { index, generation }
    }

    /// Decode from a replay unit tag (u16).
    pub fn from_tag(tag: u16) -> Self {
        Self {
            index: tag & 0x07FF,
            generation: tag >> 11,
        }
    }

    /// Encode to a replay unit tag.
    pub fn to_tag(self) -> u16 {
        (self.generation << 11) | self.index
    }

    pub fn index(self) -> u16 {
        self.index
    }

    pub fn generation(self) -> u16 {
        self.generation
    }
}

/// Simplified movement state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveState {
    #[default]
    AtRest,
    Moving,
    Arrived,
}

/// Full unit state for simulation.
#[derive(Debug, Clone)]
pub struct UnitState {
    pub id: UnitId,
    pub unit_type: u16,
    pub owner: u8,
    pub alive: bool,

    // Position
    pub exact_position: XY,
    pub pixel_x: i32,
    pub pixel_y: i32,

    // Movement
    pub velocity: XY,
    pub heading: Direction,
    pub current_speed: Fp8,
    pub move_state: MoveState,
    pub move_target: Option<(u16, u16)>,

    // Flingy params (cached from GameData)
    pub top_speed: i32,
    pub acceleration: i16,
    pub halt_distance: i32,
    pub turn_rate: u8,
    pub movement_type: u8,
}

impl UnitState {
    /// Per-frame movement update matching OpenBW's movement physics.
    pub fn update_movement(&mut self) {
        if self.move_state != MoveState::Moving {
            return;
        }
        let Some((tx, ty)) = self.move_target else {
            self.move_state = MoveState::AtRest;
            return;
        };

        let target = XY::from_pixels(tx as i32, ty as i32);
        let delta = target - self.exact_position;

        // Check arrival: if close enough, snap to target.
        let dist_sq = delta.length_squared();
        let arrival_threshold = (self.top_speed as i64).max(256) * 2;
        if dist_sq <= arrival_threshold * arrival_threshold {
            self.exact_position = target;
            self.pixel_x = tx as i32;
            self.pixel_y = ty as i32;
            self.velocity = XY::ZERO;
            self.current_speed = Fp8::ZERO;
            self.move_state = MoveState::Arrived;
            self.move_target = None;
            return;
        }

        // 1. Compute desired direction toward target.
        let desired = Direction::from_delta(delta.x, delta.y);

        // 2. Turn toward desired direction.
        // Ground units (movement_type 0) use turn_rate/2.
        let effective_rate = if self.movement_type == 2 {
            self.turn_rate
        } else {
            (self.turn_rate / 2).max(1)
        };
        self.heading = self.heading.turn_toward(desired, effective_rate);

        // 3. Accelerate or decelerate.
        let facing_target = self.heading.diff(desired).unsigned_abs() < effective_rate + 1;
        if facing_target {
            let new_speed = self.current_speed + Fp8::from_raw(self.acceleration as i32);
            self.current_speed = cap_fp8(new_speed, self.top_speed);
        } else {
            // Decelerate when not facing target.
            let new_speed = self.current_speed - Fp8::from_raw(self.acceleration as i32);
            self.current_speed = if new_speed.0 < 0 { Fp8::ZERO } else { new_speed };
        }

        // 4. Compute velocity vector from heading and current_speed.
        let (ux, uy) = self.heading.unit_vector();
        // velocity = unit_vector * speed / 256 (unit_vector has magnitude 256)
        self.velocity = XY {
            x: Fp8::from_raw(((ux.raw() as i64 * self.current_speed.raw() as i64) >> 8) as i32),
            y: Fp8::from_raw(((uy.raw() as i64 * self.current_speed.raw() as i64) >> 8) as i32),
        };

        // 5. Update position.
        self.exact_position += self.velocity;
        let (px, py) = self.exact_position.to_pixels();
        self.pixel_x = px;
        self.pixel_y = py;
    }
}

fn cap_fp8(speed: Fp8, top_speed: i32) -> Fp8 {
    if speed.raw() > top_speed {
        Fp8::from_raw(top_speed)
    } else {
        speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_unit(x: i32, y: i32) -> UnitState {
        UnitState {
            id: UnitId::new(0, 0),
            unit_type: 0,
            owner: 0,
            alive: true,
            exact_position: XY::from_pixels(x, y),
            pixel_x: x,
            pixel_y: y,
            velocity: XY::ZERO,
            heading: Direction::default(),
            current_speed: Fp8::ZERO,
            move_state: MoveState::AtRest,
            move_target: None,
            // Marine-like movement: speed ~4 px/frame, accel ~1, turn_rate ~20
            top_speed: 4 * 256, // 4.0 in fp8
            acceleration: 256,  // 1.0 in fp8
            halt_distance: 0,
            turn_rate: 40,
            movement_type: 0,
        }
    }

    #[test]
    fn test_unit_id_tag_roundtrip() {
        let id = UnitId::new(42, 3);
        let tag = id.to_tag();
        let decoded = UnitId::from_tag(tag);
        assert_eq!(decoded.index(), 42);
        assert_eq!(decoded.generation(), 3);
    }

    #[test]
    fn test_unit_id_from_tag() {
        // tag = (gen << 11) | index = (2 << 11) | 100 = 4196
        let id = UnitId::from_tag(4196);
        assert_eq!(id.index(), 100);
        assert_eq!(id.generation(), 2);
    }

    #[test]
    fn test_unit_at_rest_no_movement() {
        let mut unit = test_unit(100, 100);
        let (x0, y0) = (unit.pixel_x, unit.pixel_y);
        unit.update_movement();
        assert_eq!(unit.pixel_x, x0);
        assert_eq!(unit.pixel_y, y0);
    }

    #[test]
    fn test_unit_moves_toward_target() {
        let mut unit = test_unit(100, 100);
        unit.move_state = MoveState::Moving;
        unit.move_target = Some((300, 100)); // Move east

        // Step several frames.
        for _ in 0..50 {
            unit.update_movement();
        }

        // Unit should have moved east (x increased).
        assert!(unit.pixel_x > 100, "unit should have moved east: x={}", unit.pixel_x);
        // y should be approximately the same.
        assert!((unit.pixel_y - 100).abs() < 5, "y drift: {}", unit.pixel_y);
    }

    #[test]
    fn test_unit_arrives_at_target() {
        let mut unit = test_unit(100, 100);
        unit.move_state = MoveState::Moving;
        unit.move_target = Some((120, 100));

        // Step enough frames to reach the target.
        for _ in 0..200 {
            unit.update_movement();
            if unit.move_state == MoveState::Arrived {
                break;
            }
        }

        assert_eq!(unit.move_state, MoveState::Arrived);
        assert_eq!(unit.pixel_x, 120);
        assert_eq!(unit.pixel_y, 100);
        assert!(unit.move_target.is_none());
    }

    #[test]
    fn test_unit_stop() {
        let mut unit = test_unit(100, 100);
        unit.move_state = MoveState::Moving;
        unit.move_target = Some((300, 100));

        // Move a bit.
        for _ in 0..10 {
            unit.update_movement();
        }

        // Stop the unit.
        unit.move_state = MoveState::AtRest;
        unit.move_target = None;
        unit.velocity = XY::ZERO;
        unit.current_speed = Fp8::ZERO;

        let x_stopped = unit.pixel_x;
        for _ in 0..10 {
            unit.update_movement();
        }
        assert_eq!(unit.pixel_x, x_stopped); // Should not move.
    }

    #[test]
    fn test_speed_caps_at_top_speed() {
        let mut unit = test_unit(100, 100);
        unit.heading = Direction::EAST;
        unit.move_state = MoveState::Moving;
        unit.move_target = Some((10000, 100));

        // Step many frames — speed should cap at top_speed.
        for _ in 0..100 {
            unit.update_movement();
        }

        assert!(
            unit.current_speed.raw() <= unit.top_speed,
            "speed {} exceeded top_speed {}",
            unit.current_speed.raw(),
            unit.top_speed
        );
    }
}
