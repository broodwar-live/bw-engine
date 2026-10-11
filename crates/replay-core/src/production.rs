//! Production model: which production orders were plausibly carried out, and
//! the supply count when each order was given.
//!
//! Replays store orders, not results. A player who mashes the "Probe" hotkey
//! issues dozens of train orders, of which only as many start as the Nexus
//! queue (5 slots) and supply allow; one Zerg morph order applies to every
//! selected larva; and a morph order repeated on the same larvae does nothing.
//! This module replays the build order through a small model of those rules:
//!
//! - **Start state**: 4 workers and the main building (Command Center 10
//!   supply, Nexus 9, Hatchery 1 + Overlord 8), so supply starts at 4.
//! - **Duplicates**: identical orders by the same player within
//!   [`DUPLICATE_ORDER_FRAMES`] are UI duplicates; research, upgrades and
//!   Lair/Hive/Greater Spire morphs repeated while the first is still running
//!   ([`TECH_REPEAT_FRAMES`]) are dropped.
//! - **Queues** (Terran, Protoss): each producer holds at most
//!   [`QUEUE_SLOTS`] units; a unit starts when the one before it finishes.
//!   Orders that find every producer of their type full are dropped.
//! - **Larvae** (Zerg): each hatchery holds up to 3 larvae and spawns one every
//!   [`LARVA_SPAWN_FRAMES`]. A morph order applies to every selected larva
//!   (selection is tracked through select and hotkey commands), limited by
//!   the larvae available; repeating it on the same selection morphs only
//!   what is left of it.
//! - **Supply**: during the opening ([`SUPPLY_GATE_FRAMES`]) units start
//!   only when completed supply (depots, pylons, overlords, town halls) has
//!   room; queued units wait for the next provider to finish. Supply is
//!   cumulative (deaths are not modelled) and reported capped at 200.
//!
//! - **Minerals**: a unit order (train or larva morph) is dropped if the units
//!   ordered so far cost more than the player could have mined by then, using
//!   a generous upper bound on mining ([`MAX_MINING_PER_WORKER_FRAME`] per
//!   completed worker). Structures and tech are not checked or counted: games
//!   with non-standard starting resources exist, and a wrongly dropped
//!   structure would mislabel the opening. This only catches impossible
//!   orders; it is not an economy simulation.
//!
//! Not modelled: gas, exact income, unit
//! deaths, cancels, which building a train order went to (the least busy one
//! is assumed), and add-on/tech requirements. Supply counts are therefore
//! estimates, but they match the conventional "9 pool" / "8 pylon" /
//! "10 rax" style notation on real replays.

use crate::analysis::{BuildAction, Selection, build_orders_with_selection};
use crate::command::GameCommand;
use crate::gamedata;
use crate::header::Race;

/// Units a single production building can hold in its queue.
pub const QUEUE_SLOTS: usize = 5;
/// Frames between larva spawns per hatchery.
pub const LARVA_SPAWN_FRAMES: u32 = 342;
/// Larvae a hatchery holds at most.
pub const MAX_LARVAE: u32 = 3;
/// Identical orders by the same player within this many frames count once.
pub const DUPLICATE_ORDER_FRAMES: u32 = 3;
/// Repeated research/upgrade/tech-morph orders within this window (about one
/// real minute) are clicks on something that is already running.
pub const TECH_REPEAT_FRAMES: u32 = 1428;
/// Upper bound on minerals one worker mines per frame. Real close-patch
/// mining is about 0.045; the bound is deliberately generous so only unit
/// orders that are clearly impossible (e.g. a second Probe a few frames into
/// the game) are dropped as unaffordable.
pub const MAX_MINING_PER_WORKER_FRAME: f64 = 0.08;
/// Minerals every player starts with.
pub const STARTING_MINERALS: f64 = 50.0;
/// Supply is enforced for orders in the first six real minutes only. Unit
/// deaths are not modelled, so later on the cumulative count would wrongly
/// block production.
pub const SUPPLY_GATE_FRAMES: u32 = 8572;
/// Supply cap.
pub const MAX_SUPPLY: u32 = 200;
/// Workers every player starts with.
pub const STARTING_WORKERS: u32 = 4;

/// Why an order was judged not to have been carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DropReason {
    /// Same order repeated within a few frames, or tech already running.
    Duplicate,
    /// Every producer of this unit was already holding 5 units.
    QueueFull,
    /// No larva available (or the selected larvae were already used).
    NoLarva,
    /// Not enough supply, and none was coming.
    SupplyBlocked,
    /// No building that can produce this unit existed yet.
    NoProducer,
    /// More minerals than the player could possibly have mined by then.
    Unaffordable,
}

/// A build order entry annotated by the production model.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionEntry {
    pub frame: u32,
    pub real_seconds: f64,
    pub player_id: u8,
    pub action: BuildAction,
    /// Supply in use when the order was given (before this order), in the
    /// usual whole-number notation: "9 Pool" means `supply == 9`.
    pub supply: u32,
    /// Supply capacity provided by completed buildings/overlords at that time.
    pub supply_max: u32,
    /// How many times the order was carried out: the number of larvae for a
    /// Zerg morph (a Zergling/Scourge larva hatches a pair), otherwise 1.
    /// 0 when dropped.
    pub count: u32,
    /// Frame production actually started (after waiting in a queue or for
    /// supply). `None` when dropped.
    pub start_frame: Option<u32>,
    /// True if the order could not have been carried out.
    pub dropped: bool,
    /// Why the order was dropped.
    pub drop_reason: Option<DropReason>,
}

/// Result of running the production model over a replay.
#[derive(Debug, Clone, Default)]
pub struct ProductionModel {
    /// One entry per build order entry, in order.
    pub entries: Vec<ProductionEntry>,
    /// Per player: (player_id, starting supply capacity).
    base: Vec<(u8, u32)>,
    /// (player_id, frame, supply delta) for every started unit.
    supply_events: Vec<(u8, u32, i32)>,
    /// (player_id, frame, capacity) for every completed supply provider.
    capacity_events: Vec<(u8, u32, u32)>,
}

impl ProductionModel {
    /// Supply `(used, capacity)` of a player at `frame`, counting everything
    /// that started or completed up to and including that frame.
    #[must_use]
    pub fn supply_at(&self, player_id: u8, frame: u32) -> (u32, u32) {
        let base = self
            .base
            .iter()
            .find(|(p, _)| *p == player_id)
            .map_or(0, |(_, b)| *b);
        let used = self
            .supply_events
            .iter()
            .filter(|(p, f, _)| *p == player_id && *f <= frame)
            .fold(STARTING_WORKERS as i32, |acc, (_, _, d)| acc + d);
        let cap = self
            .capacity_events
            .iter()
            .filter(|(p, f, _)| *p == player_id && *f <= frame)
            .fold(base, |acc, (_, _, c)| acc + c);
        ((used.max(0) as u32).min(MAX_SUPPLY), cap.min(MAX_SUPPLY))
    }
}

/// Run the production model over a replay's command stream.
///
/// `players` lists the players to model with their race; orders by other
/// player ids are passed through unmodelled (supply 0, never dropped).
#[must_use]
pub fn model_production(commands: &[GameCommand], players: &[(u8, Race)]) -> ProductionModel {
    let orders = build_orders_with_selection(commands);
    // Pass 1 finds when supply providers complete (they never wait for
    // supply themselves); pass 2 uses that to gate units on supply.
    let first = run(&orders, players, None);
    let providers = first.capacity_events.clone();
    run(&orders, players, Some(&providers))
}

/// Starting supply capacity of a race.
#[must_use]
pub fn starting_capacity(race: Race) -> u32 {
    match race {
        Race::Terran => gamedata::supply_provided(COMMAND_CENTER),
        Race::Protoss => gamedata::supply_provided(NEXUS),
        Race::Zerg => gamedata::supply_provided(HATCHERY) + gamedata::supply_provided(OVERLORD),
        Race::Unknown(_) => 0,
    }
}

/// Whether the action produces a worker (SCV, Drone, Probe).
#[must_use]
pub fn is_worker_order(action: &BuildAction) -> bool {
    matches!(
        action,
        BuildAction::Train(7 | 41 | 64) | BuildAction::UnitMorph(41)
    )
}

const COMMAND_CENTER: u16 = 106;
const NEXUS: u16 = 154;
const HATCHERY: u16 = 131;
const OVERLORD: u16 = 42;

/// Building that produces a trained (non-larva) unit.
fn producer_of(unit: u16) -> Option<u16> {
    Some(match unit {
        7 => COMMAND_CENTER,
        0 | 1 | 32 | 34 => 111,      // Barracks
        2 | 3 | 5 => 113,            // Factory
        8 | 9 | 11 | 12 | 58 => 114, // Starport
        64 => NEXUS,
        61 | 65..=67 => 160, // Gateway
        69 | 83 | 84 => 155, // Robotics Facility
        60 | 70..=72 => 167, // Stargate
        _ => return None,
    })
}

/// Units morphed from larvae.
fn is_larva_unit(unit: u16) -> bool {
    matches!(unit, 37..=43 | 46 | 47)
}

/// Supply added per order. A Zergling/Scourge larva hatches a pair for 1;
/// morphs from another unit add only the difference.
fn supply_delta(action: &BuildAction) -> i32 {
    match action {
        BuildAction::UnitMorph(103) => 1,     // Hydralisk (1) -> Lurker (2)
        BuildAction::UnitMorph(44 | 62) => 0, // Mutalisk -> Guardian/Devourer
        BuildAction::Train(id) | BuildAction::UnitMorph(id) => gamedata::unit_cost(*id).2 as i32,
        _ => 0,
    }
}

/// Building morphs that exist once per base and take long: repeats while the
/// first is running are dropped.
fn is_tech_morph(unit: u16) -> bool {
    matches!(unit, 132 | 133 | 137) // Lair, Hive, Greater Spire
}

struct Producer {
    kind: u16,
    ready: u32,
    /// End frames of units queued or in production.
    queue: Vec<u32>,
}

struct Hatchery {
    ready: u32,
    larvae: u32,
    /// Frame the current larva spawn timer started.
    timer: u32,
}

impl Hatchery {
    fn update(&mut self, frame: u32) {
        if frame < self.ready {
            return;
        }
        if self.timer < self.ready {
            self.timer = self.ready;
        }
        while self.larvae < MAX_LARVAE && self.timer + LARVA_SPAWN_FRAMES <= frame {
            self.larvae += 1;
            self.timer += LARVA_SPAWN_FRAMES;
        }
        if self.larvae >= MAX_LARVAE {
            self.timer = frame;
        }
    }
}

struct PlayerModel {
    id: u8,
    race: Race,
    producers: Vec<Producer>,
    hatcheries: Vec<Hatchery>,
    /// (frame, delta) of started units.
    starts: Vec<(u32, i32)>,
    /// (ready frame, capacity) of providers, from pass 1 (or collected now).
    providers: Vec<(u32, u32)>,
    /// Last order (action, frame) for duplicate detection.
    last_order: Option<(BuildAction, u32)>,
    /// Last frame each research/upgrade/tech morph was accepted.
    last_tech: Vec<(BuildAction, u32)>,
    /// Larvae already morphed from the current selection: (generation, used).
    larva_selection: (u32, u16),
    /// Frames at which workers finished (starting workers at 0).
    workers: Vec<u32>,
    /// Minerals spent on accepted unit orders.
    spent: f64,
    /// Pass 2: enforce supply using the providers found in pass 1.
    gate_supply: bool,
}

impl PlayerModel {
    fn new(id: u8, race: Race) -> Self {
        let mut producers = Vec::new();
        let mut hatcheries = Vec::new();
        match race {
            Race::Terran => producers.push(Producer {
                kind: COMMAND_CENTER,
                ready: 0,
                queue: Vec::new(),
            }),
            Race::Protoss => producers.push(Producer {
                kind: NEXUS,
                ready: 0,
                queue: Vec::new(),
            }),
            Race::Zerg => hatcheries.push(Hatchery {
                ready: 0,
                larvae: MAX_LARVAE,
                timer: 0,
            }),
            Race::Unknown(_) => {}
        }
        Self {
            id,
            race,
            producers,
            hatcheries,
            starts: Vec::new(),
            providers: Vec::new(),
            last_order: None,
            last_tech: Vec::new(),
            larva_selection: (u32::MAX, 0),
            workers: vec![0; STARTING_WORKERS as usize],
            spent: 0.0,
            gate_supply: false,
        }
    }

    fn supply_used(&self, frame: u32) -> u32 {
        let used = self
            .starts
            .iter()
            .filter(|(f, _)| *f <= frame)
            .fold(STARTING_WORKERS as i32, |acc, (_, d)| acc + d);
        used.max(0) as u32
    }

    fn capacity(&self, frame: u32) -> u32 {
        let cap = self
            .providers
            .iter()
            .filter(|(f, _)| *f <= frame)
            .fold(starting_capacity(self.race), |acc, (_, c)| acc + c);
        cap.min(MAX_SUPPLY)
    }

    /// Earliest frame >= `from` at which `delta` more supply fits, if any.
    fn supply_room_at(&self, from: u32, delta: i32) -> Option<u32> {
        if delta <= 0 {
            return Some(from);
        }
        let fits = |f: u32| self.supply_used(f) as i32 + delta <= self.capacity(f) as i32;
        if fits(from) {
            return Some(from);
        }
        let mut readies: Vec<u32> = self
            .providers
            .iter()
            .map(|(f, _)| *f)
            .filter(|f| *f > from)
            .collect();
        readies.sort_unstable();
        readies.into_iter().find(|&f| fits(f))
    }

    /// Whether supply is enforced for an order at `frame`. Deaths are not
    /// modelled, so supply would only ever grow; it is enforced during the
    /// opening only.
    fn gates(&self, frame: u32) -> bool {
        self.gate_supply && frame < SUPPLY_GATE_FRAMES
    }

    /// Pass 1 collects supply providers as they are ordered; pass 2 uses
    /// that list.
    fn add_provider(&mut self, ready: u32, unit: u16) {
        let cap = gamedata::supply_provided(unit);
        if !self.gate_supply && cap > 0 {
            self.providers.push((ready, cap));
        }
    }

    /// Upper bound of the player's unspent minerals at `frame`.
    fn balance(&self, frame: u32) -> f64 {
        let mined: f64 = self
            .workers
            .iter()
            .map(|&ready| f64::from(frame.saturating_sub(ready)))
            .sum::<f64>()
            * MAX_MINING_PER_WORKER_FRAME;
        STARTING_MINERALS + mined - self.spent
    }

    /// Returns (count, start frame) or the reason the order was dropped.
    fn apply(
        &mut self,
        frame: u32,
        action: &BuildAction,
        selection: Selection,
    ) -> Result<(u32, u32), DropReason> {
        // Identical order repeated within a few frames.
        if let Some((last, at)) = &self.last_order
            && last == action
            && frame.saturating_sub(*at) <= DUPLICATE_ORDER_FRAMES
            && !matches!(action, BuildAction::UnitMorph(_))
        {
            return Err(DropReason::Duplicate);
        }
        self.last_order = Some((action.clone(), frame));

        let cost = match action {
            BuildAction::Train(_) | BuildAction::UnitMorph(_) => mineral_cost(action),
            _ => 0,
        };
        let affordable = if cost == 0 {
            u32::MAX
        } else {
            (self.balance(frame) / f64::from(cost)).floor().max(0.0) as u32
        };
        if affordable == 0 {
            return Err(DropReason::Unaffordable);
        }

        let (count, start) = self.apply_inner(frame, action, selection, affordable)?;
        self.spent += f64::from(cost) * f64::from(count);
        if is_worker_order(action) {
            let ready = start + gamedata::build_time(7);
            self.workers
                .extend(std::iter::repeat_n(ready, count as usize));
        }
        Ok((count, start))
    }

    fn apply_inner(
        &mut self,
        frame: u32,
        action: &BuildAction,
        selection: Selection,
        max_count: u32,
    ) -> Result<(u32, u32), DropReason> {
        let gate_supply = self.gates(frame);
        match *action {
            BuildAction::Build(unit) => {
                let ready = frame + gamedata::build_time(unit).max(1);
                if self.race == Race::Zerg {
                    // The drone becomes the building.
                    self.starts.push((frame, -1));
                    if unit == HATCHERY {
                        self.hatcheries.push(Hatchery {
                            ready,
                            larvae: 0,
                            timer: ready,
                        });
                    }
                }
                if self.race != Race::Zerg && producer_kinds().contains(&unit) {
                    self.producers.push(Producer {
                        kind: unit,
                        ready,
                        queue: Vec::new(),
                    });
                }
                self.add_provider(ready, unit);
                Ok((1, frame))
            }
            BuildAction::BuildingMorph(unit) => {
                if is_tech_morph(unit) && !self.accept_tech(action, frame) {
                    return Err(DropReason::Duplicate);
                }
                Ok((1, frame))
            }
            BuildAction::Research(_) | BuildAction::Upgrade(_) => {
                if !self.accept_tech(action, frame) {
                    return Err(DropReason::Duplicate);
                }
                Ok((1, frame))
            }
            BuildAction::TrainFighter => Ok((1, frame)),
            BuildAction::UnitMorph(unit) if is_larva_unit(unit) => {
                self.morph_larvae(frame, action, unit, selection, max_count)
            }
            BuildAction::UnitMorph(_) => {
                let delta = supply_delta(action);
                if gate_supply && self.supply_room_at(frame, delta) != Some(frame) {
                    return Err(DropReason::SupplyBlocked);
                }
                self.starts.push((frame, delta));
                Ok((1, frame))
            }
            BuildAction::Train(unit) => self.train(frame, action, unit),
        }
    }

    fn accept_tech(&mut self, action: &BuildAction, frame: u32) -> bool {
        match self.last_tech.iter_mut().find(|(a, _)| a == action) {
            Some((_, at)) if frame.saturating_sub(*at) < TECH_REPEAT_FRAMES => false,
            Some((_, at)) => {
                *at = frame;
                true
            }
            None => {
                self.last_tech.push((action.clone(), frame));
                true
            }
        }
    }

    fn morph_larvae(
        &mut self,
        frame: u32,
        action: &BuildAction,
        unit: u16,
        selection: Selection,
        max_count: u32,
    ) -> Result<(u32, u32), DropReason> {
        let gate_supply = self.gates(frame);
        if self.larva_selection.0 != selection.generation {
            self.larva_selection = (selection.generation, 0);
        }
        let from_selection = selection.count.saturating_sub(self.larva_selection.1) as u32;
        if from_selection == 0 {
            return Err(DropReason::Duplicate);
        }
        for h in &mut self.hatcheries {
            h.update(frame);
        }
        let available: u32 = self.hatcheries.iter().map(|h| h.larvae).sum();
        let mut n = from_selection.min(available).min(max_count);
        if n == 0 {
            return Err(DropReason::NoLarva);
        }
        let delta = supply_delta(action);
        if gate_supply && delta > 0 {
            let room = self.capacity(frame) as i32 - self.supply_used(frame) as i32;
            n = n.min((room.max(0) / delta) as u32);
            if n == 0 {
                return Err(DropReason::SupplyBlocked);
            }
        }

        // Take larvae from the fullest hatcheries.
        let mut left = n;
        while left > 0 {
            let Some(h) = self
                .hatcheries
                .iter_mut()
                .filter(|h| h.larvae > 0)
                .max_by_key(|h| h.larvae)
            else {
                break;
            };
            if h.larvae == MAX_LARVAE {
                h.timer = frame;
            }
            h.larvae -= 1;
            left -= 1;
        }

        self.larva_selection.1 += n as u16;
        for _ in 0..n {
            self.starts.push((frame, delta));
            let ready = frame + gamedata::build_time(unit).max(1);
            self.add_provider(ready, unit);
        }
        Ok((n, frame))
    }

    fn train(
        &mut self,
        frame: u32,
        action: &BuildAction,
        unit: u16,
    ) -> Result<(u32, u32), DropReason> {
        let gate_supply = self.gates(frame);
        let delta = supply_delta(action);
        let Some(kind) = producer_of(unit) else {
            // Unknown producer: count it as started immediately.
            self.starts.push((frame, delta));
            return Ok((1, frame));
        };

        let build = gamedata::build_time(unit).max(1);
        let mut best: Option<(usize, u32)> = None;
        let mut any_live = false;
        for (i, p) in self.producers.iter_mut().enumerate() {
            if p.kind != kind || p.ready > frame {
                continue;
            }
            any_live = true;
            p.queue.retain(|&end| end > frame);
            if p.queue.len() >= QUEUE_SLOTS {
                continue;
            }
            let free = p.queue.iter().copied().max().unwrap_or(frame).max(frame);
            if best.is_none_or(|(_, f)| free < f) {
                best = Some((i, free));
            }
        }
        let Some((idx, free)) = best else {
            return Err(if any_live {
                DropReason::QueueFull
            } else {
                DropReason::NoProducer
            });
        };

        let begin = if gate_supply {
            self.supply_room_at(free, delta)
                .ok_or(DropReason::SupplyBlocked)?
        } else {
            free
        };
        self.producers[idx].queue.push(begin + build);
        self.starts.push((begin, delta));
        self.add_provider(begin + build, unit);
        Ok((1, begin))
    }
}

/// Minerals one execution of the order costs.
fn mineral_cost(action: &BuildAction) -> u32 {
    match action {
        // gamedata lists Zergling/Scourge per unit; a larva hatches a pair.
        BuildAction::UnitMorph(id @ (37 | 47)) => gamedata::unit_cost(*id).0 * 2,
        BuildAction::UnitMorph(103) => 50, // Hydralisk -> Lurker
        BuildAction::Build(id)
        | BuildAction::Train(id)
        | BuildAction::UnitMorph(id)
        | BuildAction::BuildingMorph(id) => gamedata::unit_cost(*id).0,
        BuildAction::Research(id) => gamedata::tech_cost(*id).0,
        BuildAction::Upgrade(id) => gamedata::upgrade_cost(*id).0,
        BuildAction::TrainFighter => 15,
    }
}

fn producer_kinds() -> [u16; 8] {
    [COMMAND_CENTER, 111, 113, 114, NEXUS, 160, 155, 167]
}

fn run(
    orders: &[(crate::analysis::BuildOrderEntry, Selection)],
    players: &[(u8, Race)],
    providers: Option<&[(u8, u32, u32)]>,
) -> ProductionModel {
    let gate_supply = providers.is_some();
    let mut models: Vec<PlayerModel> = players
        .iter()
        .map(|&(id, race)| {
            let mut m = PlayerModel::new(id, race);
            m.gate_supply = gate_supply;
            if let Some(p) = providers {
                m.providers = p
                    .iter()
                    .filter(|(pid, _, _)| *pid == id)
                    .map(|&(_, f, c)| (f, c))
                    .collect();
            }
            m
        })
        .collect();

    let mut entries = Vec::with_capacity(orders.len());
    for (order, selection) in orders {
        let Some(m) = models.iter_mut().find(|m| m.id == order.player_id) else {
            entries.push(ProductionEntry {
                frame: order.frame,
                real_seconds: order.real_seconds,
                player_id: order.player_id,
                action: order.action.clone(),
                supply: 0,
                supply_max: 0,
                count: 1,
                start_frame: Some(order.frame),
                dropped: false,
                drop_reason: None,
            });
            continue;
        };
        let supply = m.supply_used(order.frame).min(MAX_SUPPLY);
        let supply_max = m.capacity(order.frame);
        let result = m.apply(order.frame, &order.action, *selection);
        let (count, start_frame, drop_reason) = match result {
            Ok((count, start)) => (count, Some(start), None),
            Err(reason) => (0, None, Some(reason)),
        };
        entries.push(ProductionEntry {
            frame: order.frame,
            real_seconds: order.real_seconds,
            player_id: order.player_id,
            action: order.action.clone(),
            supply,
            supply_max,
            count,
            start_frame,
            dropped: drop_reason.is_some(),
            drop_reason,
        });
    }

    let mut model = ProductionModel {
        entries,
        ..ProductionModel::default()
    };
    for m in &models {
        model.base.push((m.id, starting_capacity(m.race)));
        model
            .supply_events
            .extend(m.starts.iter().map(|&(f, d)| (m.id, f, d)));
        model
            .capacity_events
            .extend(m.providers.iter().map(|&(f, c)| (m.id, f, c)));
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, HotkeyAction};

    fn cmd(frame: u32, player_id: u8, command: Command) -> GameCommand {
        GameCommand {
            frame,
            player_id,
            command,
        }
    }

    fn train(frame: u32, unit_type: u16) -> GameCommand {
        cmd(frame, 0, Command::Train { unit_type })
    }

    fn morph(frame: u32, unit_type: u16) -> GameCommand {
        cmd(frame, 0, Command::UnitMorph { unit_type })
    }

    fn build(frame: u32, unit_type: u16) -> GameCommand {
        cmd(
            frame,
            0,
            Command::Build {
                order: 0x1E,
                x: 0,
                y: 0,
                unit_type,
            },
        )
    }

    fn select(frame: u32, tags: &[u16]) -> GameCommand {
        cmd(
            frame,
            0,
            Command::Select {
                unit_tags: tags.to_vec(),
            },
        )
    }

    #[test]
    fn test_model_starting_supply_counts_workers_and_main_building() {
        for (race, cap) in [(Race::Terran, 10), (Race::Protoss, 9), (Race::Zerg, 9)] {
            let m = model_production(&[], &[(0, race)]);
            assert_eq!(m.supply_at(0, 0), (4, cap), "{race:?}");
        }
    }

    #[test]
    fn test_model_probe_spam_capped_by_queue() {
        // 20 probe orders in two seconds: 5 fit in the Nexus queue.
        let cmds: Vec<GameCommand> = (0..20).map(|i| train(2010 + i * 4, 64)).collect();
        let m = model_production(&cmds, &[(0, Race::Protoss)]);
        let kept = m.entries.iter().filter(|e| !e.dropped).count();
        assert_eq!(kept, QUEUE_SLOTS);
        assert!(
            m.entries
                .iter()
                .any(|e| e.drop_reason == Some(DropReason::QueueFull))
        );
        // Queued probes start one after another, 300 frames apart.
        let starts: Vec<u32> = m.entries.iter().filter_map(|e| e.start_frame).collect();
        assert_eq!(starts, vec![2010, 2310, 2610, 2910, 3210]);
        assert_eq!(m.supply_at(0, 2010).0, 5);
        assert_eq!(m.supply_at(0, 2700).0, 7);
    }

    #[test]
    fn test_model_duplicate_train_within_few_frames_counts_once() {
        let cmds = vec![train(100, 7), train(102, 7), train(200, 7)];
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        let dropped: Vec<bool> = m.entries.iter().map(|e| e.dropped).collect();
        assert_eq!(dropped, vec![false, true, false]);
        assert_eq!(m.entries[1].drop_reason, Some(DropReason::Duplicate));
    }

    #[test]
    fn test_model_supply_block_delays_queued_units() {
        // Protoss 9/9: the 6th probe waits for the pylon to finish.
        let mut cmds: Vec<GameCommand> = (0..5).map(|i| train(i * 300, 64)).collect();
        cmds.push(build(1200, 156)); // Pylon, ready at 1650
        cmds.push(train(1300, 64));
        let m = model_production(&cmds, &[(0, Race::Protoss)]);
        // 4 + 5 probes = 9/9 at frame 1200.
        assert_eq!(m.supply_at(0, 1200), (9, 9));
        let last = m.entries.last().unwrap();
        assert_eq!(last.supply, 9);
        assert_eq!(last.supply_max, 9);
        // The queue frees up at 1500, but supply only at 1650.
        assert_eq!(last.start_frame, Some(1650));
        assert_eq!(m.supply_at(0, 1600), (9, 9));
        assert_eq!(m.supply_at(0, 1650), (10, 17));
    }

    #[test]
    fn test_model_supply_blocked_without_provider_drops() {
        let cmds: Vec<GameCommand> = (0..8).map(|i| train(i * 300, 7)).collect();
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        // 4 SCVs + 6 = 10/10; the last two never get supply.
        let kept = m.entries.iter().filter(|e| !e.dropped).count();
        assert_eq!(kept, 6);
        assert_eq!(
            m.entries.last().unwrap().drop_reason,
            Some(DropReason::SupplyBlocked)
        );
    }

    #[test]
    fn test_model_multi_larva_morph_counts_selected_larvae() {
        let cmds = vec![select(2005, &[1, 2, 3]), morph(2010, 41)];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        assert_eq!(m.entries[0].count, 3);
        assert_eq!(m.supply_at(0, 2010).0, 7);
    }

    #[test]
    fn test_model_repeated_morph_on_same_larvae_is_duplicate() {
        // Pressing "Drone" again on the same (now egg) selection does nothing.
        let cmds = vec![
            select(2005, &[1]),
            morph(2010, 41),
            morph(2014, 41),
            morph(2040, 41),
        ];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        let counts: Vec<u32> = m.entries.iter().map(|e| e.count).collect();
        assert_eq!(counts, vec![1, 0, 0]);
        assert_eq!(m.entries[1].drop_reason, Some(DropReason::Duplicate));
    }

    #[test]
    fn test_model_larvae_limited_by_hatchery() {
        // 3 starting larvae; after using them the next spawns 342 frames later.
        let cmds = vec![
            select(2001, &[1, 2, 3, 4, 5]),
            morph(2010, 41),
            select(2020, &[6]),
            morph(2030, 41),
            select(2400, &[7]),
            morph(2400, 41),
        ];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        let counts: Vec<u32> = m.entries.iter().map(|e| e.count).collect();
        assert_eq!(counts, vec![3, 0, 1]);
        assert_eq!(m.entries[1].drop_reason, Some(DropReason::NoLarva));
    }

    #[test]
    fn test_model_zerg_building_consumes_drone() {
        let cmds = vec![
            select(1, &[1]),
            morph(10, 41),
            select(400, &[2]),
            morph(400, 41),
            build(900, 142),
            build(1000, 149),
        ];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        let pool = &m.entries[2];
        assert_eq!(pool.supply, 6);
        assert_eq!(m.entries[3].supply, 5);
    }

    #[test]
    fn test_model_research_spam_dropped() {
        let cmds = vec![
            cmd(3000, 0, Command::Research { tech_type: 0 }),
            cmd(3050, 0, Command::Research { tech_type: 0 }),
            cmd(
                3000 + TECH_REPEAT_FRAMES,
                0,
                Command::Research { tech_type: 0 },
            ),
        ];
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        let dropped: Vec<bool> = m.entries.iter().map(|e| e.dropped).collect();
        assert_eq!(dropped, vec![false, true, false]);
    }

    #[test]
    fn test_model_train_without_producer_dropped() {
        let cmds = vec![train(100, 0)]; // Marine without Barracks
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        assert_eq!(m.entries[0].drop_reason, Some(DropReason::NoProducer));
        // Barracks finished: Marine accepted.
        let cmds = vec![build(2100, 111), train(3400, 0)];
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        assert!(!m.entries[1].dropped);
    }

    #[test]
    fn test_model_hotkey_recall_starts_new_larva_selection() {
        let cmds = vec![
            select(1, &[1]),
            cmd(
                2,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Assign,
                    group: 1,
                },
            ),
            morph(10, 41),
            cmd(
                400,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Select,
                    group: 1,
                },
            ),
            morph(410, 41),
        ];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        let counts: Vec<u32> = m.entries.iter().map(|e| e.count).collect();
        assert_eq!(counts, vec![1, 1]);
    }

    #[test]
    fn test_model_larva_morph_limited_by_minerals() {
        // Three larvae selected at the start, but only 50 minerals: one Drone.
        let cmds = vec![select(1, &[1, 2, 3]), morph(5, 41)];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        assert_eq!(m.entries[0].count, 1);
        // Nothing left a few frames later.
        let cmds = vec![select(1, &[1]), morph(5, 41), select(8, &[2]), morph(9, 41)];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        assert_eq!(m.entries[1].drop_reason, Some(DropReason::Unaffordable));
    }

    #[test]
    fn test_model_structures_not_checked_for_minerals() {
        // Non-standard starting resources exist; structures are never dropped
        // for money.
        let cmds = vec![build(100, 142), build(200, 131), build(300, 131)];
        let m = model_production(&cmds, &[(0, Race::Zerg)]);
        assert!(m.entries.iter().all(|e| !e.dropped));
        assert_eq!(m.supply_at(0, 300).0, 1);
    }

    #[test]
    fn test_model_starting_minerals_limit_first_orders() {
        // 50 minerals at frame 0: the second SCV order a few frames later
        // cannot be paid for yet.
        let cmds = vec![train(5, 7), train(9, 7)];
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        assert!(!m.entries[0].dropped);
        assert_eq!(m.entries[1].drop_reason, Some(DropReason::Unaffordable));
    }

    #[test]
    fn test_model_unknown_player_passed_through() {
        let cmds = vec![cmd(10, 9, Command::Train { unit_type: 7 })];
        let m = model_production(&cmds, &[(0, Race::Terran)]);
        assert!(!m.entries[0].dropped);
        assert_eq!(m.entries[0].count, 1);
    }
}
