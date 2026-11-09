//! Per-frame script dispatch (stage 0b).
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
use crate::sim::intern::InternedId;
use crate::sim::world::{FrameEffects, LifecycleOutput, Simulation};

use super::{script_registry, ScriptContext, ScriptEvent, SimCommand};

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
        let mut bound: Vec<(u64, InternedId)> = Vec::new();
        for entity in self.substrate.entities.values() {
            let Some(object) = self.interner.object_type(entity.type_ref(), rules) else {
                continue;
            };
            let Some(binding) = object.script.as_deref() else {
                continue;
            };
            if registry.binding_for(binding).is_some() {
                bound.push((entity.stable_id(), entity.owner()));
            }
        }
        bound.sort_unstable_by_key(|(id, _)| *id);

        for (entity, owner) in bound {
            let event = ScriptEvent::Update { entity, frame };
            let ScriptContext {
                commands, lasers, ..
            } = registry.dispatch(&event, &mut self.script_states, rules, frame);

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
                        self.fire_super_weapon(
                            rules,
                            owner,
                            super_weapon,
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
                        // Stage 1 wires this through
                        // `combat::world_receiver::apply_area_damage`. Its
                        // `origin` triple and `affects_tiberium` flag must be
                        // pinned against a real script (the first one is the
                        // GBION show) before damage is allowed to reach the
                        // world; silently guessing them here would make
                        // replays diverge. Until then this is an explicit,
                        // structured diagnostic rather than a silent drop.
                        log::debug!(
                            "script command ApplyWarhead deferred (warhead={:?}, damage={}, coord={:?}); \
                             stage 1 will route it through the receiver chain",
                            warhead,
                            damage,
                            coord
                        );
                    }
                }
            }
        }
    }
}
