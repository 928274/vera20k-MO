//! Per-frame script dispatch (stage 0b / stage 1).
//!
//! Runs at a fixed point in `advance_master_frame`, after the ordered logic
//! steps and before the late region. Everything here is deterministic: the
//! registry order, the stable-id order of entities and the command order are
//! all fixed, so script state belongs in the state hash and in replays.
//!
//! With an empty registry this returns immediately, so a build with no
//! scripts pays no per-frame cost and behaves byte-for-byte as before.

use crate::rules::overlay_types::OverlayTypeRegistry;
use crate::rules::ruleset::RuleSet;
use crate::sim::combat::world_receiver::apply_area_damage;
use crate::sim::combat::RAD_NO_ATTACKER;
use crate::sim::intern::InternedId;
use crate::sim::projectile::ProjectileCoord;
use crate::sim::world::{FrameEffects, LifecycleOutput, Simulation};

use super::{script_registry, ScriptContext, ScriptEvent, SimCommand};

/// 256 leptons per cell; `Position::rx/ry` are cell coordinates.
const LEPTONS_PER_CELL: i32 = 256;

impl Simulation {
    /// Dispatch one `Update` per entity whose TechnoType declares a registered
    /// `Script=` binding, then execute the resulting side effects in order.
    pub(crate) fn dispatch_script_frame(
        &mut self,
        rules: &RuleSet,
        overlay_registry: Option<&OverlayTypeRegistry>,
        effects: FrameEffects<'_>,
    ) {
        let registry = script_registry();
        if registry.is_empty() {
            return;
        }
        let frame = self.session.binary_frame() as i32;

        // Collect first: dispatch mutates `script_states` and may emit into
        // `lifecycle_outputs`, so the read-only entity walk must finish before
        // any of that starts. Stable-id order is part of the determinism
        // contract.
        let mut bound: Vec<(u64, InternedId, (i32, i32, i32))> = Vec::new();
        for entity in self.substrate.entities.values() {
            let Some(object) = self.interner.object_type(entity.type_ref(), rules) else {
                continue;
            };
            let Some(binding) = object.script.as_deref() else {
                continue;
            };
            if registry.binding_for(binding).is_some() {
                let position = &entity.position;
                let anchor = (
                    position.rx() as i32 * LEPTONS_PER_CELL,
                    position.ry() as i32 * LEPTONS_PER_CELL,
                    position.z() as i32,
                );
                bound.push((entity.stable_id(), entity.owner(), anchor));
            }
        }
        bound.sort_unstable_by_key(|(id, _, _)| *id);

        for (entity, owner, anchor) in bound {
            let mut ctx = ScriptContext::new(rules, frame);
            ctx.anchor = anchor;
            ctx.has_target = true;
            let event = ScriptEvent::Update { entity, frame };
            let ScriptContext {
                commands, lasers, ..
            } = registry.dispatch_with(&event, &mut self.script_states, ctx);

            // Pure presentation. The sim keeps no handle to the laser, so a
            // dropped frame cannot desynchronise replays.
            for birth in lasers {
                self.lifecycle_outputs
                    .emit(LifecycleOutput::LaserCreated(birth));
            }

            for command in commands {
                match command {
                    SimCommand::FireSuperWeapon {
                        super_weapon,
                        cell,
                    } => {
                        let sw_id = self.interner.intern(&super_weapon);
                        self.fire_super_weapon(
                            rules,
                            owner,
                            sw_id,
                            cell,
                            overlay_registry,
                            effects,
                            true,
                        );
                    }
                    SimCommand::ApplyWarhead {
                        warhead,
                        damage,
                        coord,
                    } => {
                        // Same call shape as AnimClass's own DetonateAt path:
                        // (RAD_NO_ATTACKER, source house, warhead id).
                        let Some(warhead_type) = rules.warhead(&warhead) else {
                            log::debug!("script warhead '{warhead}' is not registered; skipped");
                            continue;
                        };
                        let warhead_id = self.interner.intern(&warhead);
                        apply_area_damage(
                            self,
                            rules,
                            overlay_registry,
                            ProjectileCoord::new(coord.0, coord.1, coord.2),
                            damage,
                            warhead_type,
                            true,
                            (RAD_NO_ATTACKER, Some(owner), warhead_id),
                            effects,
                        );
                    }
                }
            }
        }
    }
}
