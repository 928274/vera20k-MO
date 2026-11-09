//! First native script: the multi-act ion-cannon presentation.
//!
//! Ported from the DynamicPatcher C# `TechnoScriptable` of the same name.
//! Everything that used a pointer, a magic address or a per-frame poll in the
//! C# version is now a typed event or a `ScriptContext` call. Script state is
//! plain serde data in `ScriptState`, so it is saved, hashed and replayed.

use serde::{Deserialize, Serialize};

use crate::sim::combat::laser::{LaserBirth, LaserColor};
use crate::sim::projectile::ProjectileCoord;
use crate::util::native_x87::NativeF32Bits;

use super::{EventMask, Script, ScriptContext, ScriptEvent, ScriptId, ScriptState};

pub struct GbionIonShow;

impl Script for GbionIonShow {
    fn id(&self) -> ScriptId {
        ScriptId(1)
    }

    fn binding_name(&self) -> &str {
        "GBIONIonShowScript"
    }

    fn interests(&self) -> EventMask {
        EventMask::of(EventMask::UPDATE | EventMask::BULLET_DETONATED)
    }

    fn on_event(&self, event: &ScriptEvent, state: &mut ScriptState, ctx: &mut ScriptContext<'_>) {
        let mut show: Show = serde_json::from_slice(&state.0).unwrap_or_default();

        match event {
            ScriptEvent::BulletDetonated { .. } => {
                // The original script polls BulletClass.Array for its AAW
                // projectiles disappearing, then triggers after two hits. The
                // sim now delivers the detonation directly.
                if !show.active && show.quake_hits < QUAKE_HITS_TO_SHOW {
                    show.quake_hits += 1;
                }
            }
            ScriptEvent::Update { .. } => {
                if !show.active {
                    // Stage-1 fallback while BulletDetonated dispatch is still
                    // being wired: keep the presentation observable on units
                    // that never fire a projectile the sim reports.
                    if ctx.has_target {
                        show.idle_frames += 1;
                    }
                    let ready = show.quake_hits >= QUAKE_HITS_TO_SHOW
                        || show.idle_frames >= FALLBACK_TRIGGER_FRAMES;
                    if ready {
                        start(&mut show);
                    }
                } else if show.delay > 0 {
                    show.delay -= 1;
                } else {
                    match show.stage {
                        1 => beams(&mut show, ctx),
                        2 => charge(&mut show, ctx),
                        3 => release(&mut show, ctx),
                        _ => reset(&mut show),
                    }
                }
            }
            _ => {}
        }

        state.0 = serde_json::to_vec(&show).unwrap_or_default();
    }
}

const QUAKE_HITS_TO_SHOW: i32 = 2;
const FALLBACK_TRIGGER_FRAMES: i32 = 90;
const BLUE_INNER: [u8; 3] = [0, 35, 180];
const BLUE_SPREAD: [u8; 3] = [0, 20, 90];
const BEAM_HEIGHT: i32 = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Show {
    active: bool,
    stage: u8,
    delay: i32,
    radius: i32,
    angle: i32,
    line_speed: i32,
    rot_speed: i32,
    ready: i32,
    charge: i32,
    beams: [bool; 8],
    quake_hits: i32,
    idle_frames: i32,
}

impl Default for Show {
    fn default() -> Self {
        Self {
            active: false,
            stage: 0,
            delay: 0,
            radius: 166,
            angle: -180,
            line_speed: 24,
            rot_speed: 8,
            ready: 0,
            charge: 0,
            beams: [false; 8],
            quake_hits: 0,
            idle_frames: 0,
        }
    }
}

fn start(show: &mut Show) {
    show.active = true;
    show.stage = 1;
    show.delay = 10;
    show.radius = 166;
    show.angle = -180;
    show.line_speed = 24;
    show.rot_speed = 8;
    show.ready = 0;
    show.charge = 0;
    show.beams = [false; 8];
    show.quake_hits = 0;
    show.idle_frames = 0;
}

fn reset(show: &mut Show) {
    show.active = false;
    show.stage = 0;
    show.idle_frames = 0;
}

fn emit_beam(ctx: &mut ScriptContext<'_>, from: (i32, i32, i32), to: (i32, i32, i32), width: i32) {
    ctx.emit_laser(LaserBirth {
        frame: ctx.frame,
        from: ProjectileCoord::new(from.0, from.1, from.2),
        to: ProjectileCoord::new(to.0, to.1, to.2),
        z_adjust: 0,
        duration: 2,
        width,
        supported: false,
        color: LaserColor::Explicit {
            inner: BLUE_INNER,
            outer: BLUE_INNER,
            spread: BLUE_SPREAD,
        },
        fade_start: NativeF32Bits::ONE,
        fade_end: NativeF32Bits::POSITIVE_ZERO,
    });
}

/// Act 1: eight beams light up in a staggered order, then the ring rotates
/// and contracts while the centre keeps taking low damage.
fn beams(show: &mut Show, ctx: &mut ScriptContext<'_>) {
    if show.ready <= 60 {
        show.ready += 2;
        show.beams[0] = true;
        if show.ready >= 15 {
            show.beams[1] = true;
        }
        if show.ready >= 30 {
            show.beams[3] = true;
        }
        if show.ready >= 40 {
            show.beams[2] = true;
        }
        if show.ready >= 45 {
            show.beams[5] = true;
        }
        if show.ready >= 55 {
            show.beams[7] = true;
        }
        if show.ready >= 60 {
            show.beams[6] = true;
            show.beams[4] = true;
        }
    }

    let (cx, cy, cz) = ctx.anchor;
    let floor = cz;
    let mut index = 0usize;
    for step in 0..8 {
        let degrees = show.angle + step * 45;
        let radians = (degrees as f64).to_radians();
        let dx = (show.radius as f64 * radians.cos()) as i32;
        let dy = (show.radius as f64 * radians.sin()) as i32;
        if show.beams[index] {
            let x = cx + dx;
            let y = cy + dy;
            emit_beam(
                ctx,
                (x, y, cz + BEAM_HEIGHT),
                (x, y, floor),
                4,
            );
            ctx.apply_warhead(
                "GBIONBeamLowWH",
                if show.ready > 60 { 120 } else { 20 },
                (x, y, floor),
            );
        }
        index += 1;
    }

    if show.ready >= 60 {
        if show.ready % 5 == 0 {
            show.line_speed += 1;
            show.rot_speed += 1;
        }
        show.radius -= show.line_speed;
        show.angle -= show.rot_speed;
        ctx.apply_warhead("GBIONBeamLowWH", 100, (cx, cy, floor));
        if show.radius < 0 {
            show.stage = 2;
        }
    }
}

/// Act 2: charge, with the zero-damage charge warhead at frame 5.
fn charge(show: &mut Show, ctx: &mut ScriptContext<'_>) {
    if show.charge == 5 {
        let (x, y, z) = ctx.anchor;
        ctx.apply_warhead("GBIONChargeWH", 0, (x, y, z));
    }
    show.charge += 1;
    if show.charge >= 20 {
        show.stage = 3;
    }
}

/// Act 3: lighting superweapon, a 3x3 bundle of thick beams, and the main wave.
fn release(show: &mut Show, ctx: &mut ScriptContext<'_>) {
    let (cx, cy, cz) = ctx.anchor;
    let floor = cz;

    // Coord2Cell(center): 256 leptons per cell in VERA's world space.
    let cell = ((cx / 256).clamp(0, u16::MAX as i32) as u16, (cy / 256).clamp(0, u16::MAX as i32) as u16);
    ctx.fire_super_weapon("GBIONBlueLight", cell);

    const OFFSETS: [i32; 3] = [-20, 0, 20];
    for ox in OFFSETS {
        for oy in OFFSETS {
            let x = cx + ox;
            let y = cy + oy;
            emit_beam(ctx, (x, y, cz + BEAM_HEIGHT), (x, y, floor), 40);
        }
    }

    if ctx.is_air_target {
        ctx.apply_warhead("GBIONWaveAirWH", 700, (cx, cy, cz));
    }
    ctx.apply_warhead("GBIONWaveWH", 700, (cx, cy, floor));

    show.stage = 4;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ruleset::RuleSet;
    use crate::sim::scripting::{ScriptRegistry, ScriptStateStore, SimCommand};

    #[test]
    fn show_advances_through_all_four_acts_and_resets() {
        let rules = RuleSet::empty_for_test();
        let mut registry = ScriptRegistry::new();
        registry.register(Box::new(GbionIonShow));
        let mut states = ScriptStateStore::default();
        let entity = 7u64;

        let mut saw_lasers = false;
        let mut saw_warhead = false;
        let mut saw_super = false;
        for frame in 0..400 {
            let mut ctx = ScriptContext::new(&rules, frame);
            ctx.has_target = true;
            ctx.anchor = (1000, 2000, 0);
            let out = registry.dispatch_with(
                &ScriptEvent::Update { entity, frame },
                &mut states,
                ctx,
            );
            saw_lasers |= !out.lasers.is_empty();
            saw_warhead |= out
                .commands
                .iter()
                .any(|c| matches!(c, SimCommand::ApplyWarhead { .. }));
            saw_super |= out
                .commands
                .iter()
                .any(|c| matches!(c, SimCommand::FireSuperWeapon { .. }));
        }
        assert!(saw_lasers, "act 1/3 must emit laser births");
        assert!(saw_warhead, "acts must apply warheads");
        assert!(saw_super, "act 3 must launch the lighting superweapon");
        assert_eq!(registry.binding_for("gbionionshowscript"), Some(ScriptId(1)));
    }
}
