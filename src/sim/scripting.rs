//! Sim-layer scripting extension points (stage 0).
//!
//! This is the engine-side half of the scripting extension described in
//! `docs/scripting-extension.md`. Scripts run inside the deterministic
//! simulation: their state is part of `Simulation` (serde + state hash) and
//! every visual side effect is emitted as data for the presentation layer.
//! Nothing here holds a host pointer, reads wall-clock time, or calls a
//! renderer directly.
//!
//! Stage 0 delivers the types, the registry and the state store. Automatic
//! per-frame dispatch from `advance_master_frame` is stage 0b; until then the
//! host can call `ScriptRegistry::dispatch` explicitly, which keeps the
//! existing frame behaviour byte-for-byte unchanged.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::rules::ruleset::RuleSet;
use crate::sim::combat::laser::LaserBirth;
use crate::sim::intern::InternedId;

/// Deterministic stable id of a Techno entity (matches `GameEntity::stable_id`).
pub type EntityId = u64;

/// A stable script identity. Written into saves, so a published id must never
/// be reused for a different script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ScriptId(pub u32);

/// Bit mask of the events a script wants. Dispatch skips scripts that did not
/// subscribe, so a large script set does not wake on every entity every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventMask(pub u32);

impl EventMask {
    pub const CONSTRUCTED: u32 = 1 << 0;
    pub const UPDATE: u32 = 1 << 1;
    pub const WEAPON_FIRED: u32 = 1 << 2;
    pub const BULLET_DETONATED: u32 = 1 << 3;
    pub const DAMAGED: u32 = 1 << 4;
    pub const DESTROYED: u32 = 1 << 5;

    pub const fn of(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// The events the sim offers to scripts. All payloads are plain data so the
/// same event can cross a WASM boundary in stage 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptEvent {
    Constructed {
        entity: EntityId,
    },
    Update {
        entity: EntityId,
        frame: i32,
    },
    WeaponFired {
        entity: EntityId,
        weapon: InternedId,
        target: EntityId,
    },
    BulletDetonated {
        owner: EntityId,
        projectile: InternedId,
    },
    Damaged {
        entity: EntityId,
        warhead: InternedId,
        damage: i32,
    },
    Destroyed {
        entity: EntityId,
    },
}

impl ScriptEvent {
    pub const fn entity(&self) -> EntityId {
        match *self {
            Self::Constructed { entity }
            | Self::Update { entity, .. }
            | Self::WeaponFired { entity, .. }
            | Self::BulletDetonated { owner: entity, .. }
            | Self::Damaged { entity, .. }
            | Self::Destroyed { entity } => entity,
        }
    }

    pub const fn bit(&self) -> u32 {
        match self {
            Self::Constructed { .. } => EventMask::CONSTRUCTED,
            Self::Update { .. } => EventMask::UPDATE,
            Self::WeaponFired { .. } => EventMask::WEAPON_FIRED,
            Self::BulletDetonated { .. } => EventMask::BULLET_DETONATED,
            Self::Damaged { .. } => EventMask::DAMAGED,
            Self::Destroyed { .. } => EventMask::DESTROYED,
        }
    }
}

/// A script's per-entity state. Opaque bytes: stage 0 native scripts choose
/// their own encoding, stage 2 WASM modules own theirs. The host only promises
/// byte-for-byte persistence and deterministic iteration order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptState(pub Vec<u8>);

/// All script states in a simulation, keyed by (entity, script).
///
/// `BTreeMap` keeps iteration and hashing order independent of insertion
/// history, which is required for save/replay and multiplayer determinism.
/// Unknown script ids are preserved on load but are not dispatched once their
/// script is removed from the registry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptStateStore {
    slots: BTreeMap<(EntityId, u32), Vec<u8>>,
}

impl ScriptStateStore {
    pub fn get(&self, entity: EntityId, script: ScriptId) -> Option<&[u8]> {
        self.slots.get(&(entity, script.0)).map(Vec::as_slice)
    }

    pub fn set(&mut self, entity: EntityId, script: ScriptId, bytes: Vec<u8>) {
        self.slots.insert((entity, script.0), bytes);
    }

    pub fn take(&mut self, entity: EntityId, script: ScriptId) -> Option<Vec<u8>> {
        self.slots.remove(&(entity, script.0))
    }

    /// Called when an entity leaves the world. Script state must not outlive
    /// its owner, otherwise a reused stable id could inherit stale bytes.
    pub fn remove_entity(&mut self, entity: EntityId) {
        self.slots
            .retain(|(slot_entity, _), _| *slot_entity != entity);
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// FNV-1a over the ordered slots. Self-contained and stable across Rust
    /// versions, unlike `std::collections::hash_map::DefaultHasher`.
    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut fold = |byte: u8| {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        };
        for ((entity, script), bytes) in &self.slots {
            for byte in entity.to_le_bytes() {
                fold(byte);
            }
            for byte in script.to_le_bytes() {
                fold(byte);
            }
            for byte in (bytes.len() as u64).to_le_bytes() {
                fold(byte);
            }
            for byte in bytes {
                fold(*byte);
            }
        }
        hash
    }
}

/// A script requests work through the context; the host executes commands
/// after dispatch in a fixed order. Scripts never mutate the world directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimCommand {
    ApplyWarhead {
        warhead: InternedId,
        damage: i32,
        coord: (i32, i32, i32),
    },
    FireSuperWeapon {
        super_weapon: InternedId,
        cell: (u16, u16),
    },
}

/// What a script is allowed to observe and request during one event.
pub struct ScriptContext<'a> {
    pub rules: &'a RuleSet,
    pub frame: i32,
    /// Deterministic side effects, executed by the host after dispatch.
    pub commands: Vec<SimCommand>,
    /// Presentation-only side effects, emitted into the frame channel.
    pub lasers: Vec<LaserBirth>,
}

impl<'a> ScriptContext<'a> {
    pub fn new(rules: &'a RuleSet, frame: i32) -> Self {
        Self {
            rules,
            frame,
            commands: Vec::new(),
            lasers: Vec::new(),
        }
    }

    /// Replaces the DP script's `GameCreateLaser @ 0x6FD3D0` call. The host
    /// turns this into `LifecycleOutput::LaserCreated`, which
    /// `app/presentation/lasers.rs` consumes.
    pub fn emit_laser(&mut self, birth: LaserBirth) {
        self.lasers.push(birth);
    }

    /// Replaces `CreateBullet(...).Detonate(coord)`. Resolved against the
    /// frozen `RuleSet`, so an unknown warhead is a no-op instead of a crash.
    pub fn apply_warhead(
        &mut self,
        warhead: InternedId,
        damage: i32,
        coord: (i32, i32, i32),
    ) {
        self.commands.push(SimCommand::ApplyWarhead {
            warhead,
            damage,
            coord,
        });
    }

    /// Reaches `Simulation::fire_super_weapon` through a host command.
    pub fn fire_super_weapon(&mut self, super_weapon: InternedId, cell: (u16, u16)) {
        self.commands.push(SimCommand::FireSuperWeapon {
            super_weapon,
            cell,
        });
    }
}

/// Implemented by every native script. Stage 2 provides an adapter that
/// implements this trait by calling a WASM module with the same ABI.
pub trait Script: Send + Sync {
    fn id(&self) -> ScriptId;
    fn interests(&self) -> EventMask;

    /// `state` is the calling entity's slot; it is written back to the store
    /// only if the script changed it.
    fn on_event(&self, event: &ScriptEvent, state: &mut ScriptState, ctx: &mut ScriptContext<'_>);
}

/// Compile-time registry. Dispatch order is registration order, which is part
/// of the deterministic contract; never sort by name or by hash here.
pub struct ScriptRegistry {
    scripts: Vec<Box<dyn Script>>,
}

impl Default for ScriptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptRegistry {
    pub const fn new() -> Self {
        Self {
            scripts: Vec::new(),
        }
    }

    pub fn register(&mut self, script: Box<dyn Script>) {
        let id = script.id();
        assert!(
            !self.scripts.iter().any(|existing| existing.id() == id),
            "duplicate ScriptId"
        );
        self.scripts.push(script);
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    /// Dispatch one event to every subscribed script owning a state slot (or
    /// creating one on `Constructed`). Returns the collected context so the
    /// host can execute commands and emit presentation outputs in order.
    pub fn dispatch<'r>(
        &self,
        event: &ScriptEvent,
        states: &mut ScriptStateStore,
        rules: &'r RuleSet,
        frame: i32,
    ) -> ScriptContext<'r> {
        let mut ctx = ScriptContext::new(rules, frame);
        if event.bit() == EventMask::DESTROYED {
            states.remove_entity(event.entity());
            return ctx;
        }
        for script in &self.scripts {
            if !script.interests().contains(event.bit()) {
                continue;
            }
            let mut state = ScriptState(
                states
                    .get(event.entity(), script.id())
                    .unwrap_or_default()
                    .to_vec(),
            );
            script.on_event(event, &mut state, &mut ctx);
            states.set(event.entity(), script.id(), state.0);
        }
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter;

    impl Script for Counter {
        fn id(&self) -> ScriptId {
            ScriptId(7)
        }

        fn interests(&self) -> EventMask {
            EventMask::of(EventMask::UPDATE | EventMask::CONSTRUCTED)
        }

        fn on_event(
            &self,
            event: &ScriptEvent,
            state: &mut ScriptState,
            _ctx: &mut ScriptContext<'_>,
        ) {
            if matches!(event, ScriptEvent::Update { .. }) {
                let ticks = state.0.first().copied().unwrap_or(0).wrapping_add(1);
                state.0 = vec![ticks];
            }
        }
    }

    #[test]
    fn dispatch_updates_only_subscribed_entities_and_persists_state() {
        let rules = RuleSet::empty_for_test();
        let mut registry = ScriptRegistry::new();
        registry.register(Box::new(Counter));
        let mut states = ScriptStateStore::default();

        let update = ScriptEvent::Update {
            entity: 42,
            frame: 10,
        };
        registry.dispatch(&update, &mut states, &rules, 10);
        registry.dispatch(&update, &mut states, &rules, 11);
        assert_eq!(states.get(42, ScriptId(7)), Some(&[2u8][..]));

        // A different entity has its own slot.
        registry.dispatch(
            &ScriptEvent::Update {
                entity: 43,
                frame: 12,
            },
            &mut states,
            &rules,
            12,
        );
        assert_eq!(states.get(43, ScriptId(7)), Some(&[1u8][..]));

        // Destroying an entity clears every script slot it owned.
        let before = states.len();
        registry.dispatch(
            &ScriptEvent::Destroyed { entity: 42 },
            &mut states,
            &rules,
            13,
        );
        assert_eq!(states.len(), before - 1);
        assert_eq!(states.get(42, ScriptId(7)), None);
    }

    #[test]
    fn checksum_is_order_independent_and_sensitive_to_state() {
        let mut a = ScriptStateStore::default();
        a.set(2, ScriptId(1), vec![1, 2]);
        a.set(1, ScriptId(2), vec![3]);
        let mut b = ScriptStateStore::default();
        b.set(1, ScriptId(2), vec![3]);
        b.set(2, ScriptId(1), vec![1, 2]);
        assert_eq!(a.checksum(), b.checksum());

        b.set(2, ScriptId(1), vec![1, 2, 3]);
        assert_ne!(a.checksum(), b.checksum());
    }

    #[test]
    fn commands_and_lasers_are_collected_without_touching_the_world() {
        let rules = RuleSet::empty_for_test();
        let mut ctx = ScriptContext::new(&rules, 5);
        ctx.apply_warhead(InternedId::default(), 100, (1, 2, 3));
        ctx.fire_super_weapon(InternedId::default(), (4, 5));
        assert_eq!(ctx.commands.len(), 2);
        assert!(ctx.lasers.is_empty());
        assert_eq!(ctx.frame, 5);
    }
}
