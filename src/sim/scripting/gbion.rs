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
            ScriptEvent::BulletDetonated { impact, .. } => {
                // The original polled `BulletClass.Array` for its own AAW
                // shells leaving the active list; the sim reports that same
                // moment here. Two hits on one target start the show.
                if !show.active
                    && ctx
                        .projectile
                        .is_some_and(|name| name.eq_ignore_ascii_case(QUAKE_PROJECTILE))
                {
                    if show.quake_target != ctx.target_entity {
                        show.quake_hits = 0;
                        show.quake_target = ctx.target_entity;
                    }
                    if ctx.has_target {
                        latch(&mut show, ctx);
                    } else {
                        // No live target: the shell's own impact is the only
                        // truthful centre, and it is never the caster.
                        show.center = *impact;
                        show.height = 0;
                        show.air = false;
                    }
                    show.quake_hits += 1;
                    if show.quake_hits >= QUAKE_HITS_TO_SHOW {
                        start(&mut show);
                    }
                }
            }
            ScriptEvent::Update { .. } => {
                if show.active {
                    // Every frame re-reads `Owner.Ref.Target`: the show walks
                    // with the target, and keeps its last centre if the target
                    // is gone. It never falls back to the caster.
                    if ctx.has_target {
                        latch(&mut show, ctx);
                    }
                    if show.delay > 0 {
                        show.delay -= 1;
                    } else {
                        match show.stage {
                            1 => beams(&mut show, ctx),
                            2 => charge(&mut show, ctx),
                            3 => release(&mut show, ctx),
                            _ => reset(&mut show),
                        }
                    }
                } else {
                    // The quake count does not span two targets.
                    if show.quake_target != ctx.target_entity {
                        show.quake_hits = 0;
                        show.quake_target = ctx.target_entity;
                    }
                    // The quake gun cannot reach aircraft, so a flying target
                    // held for AIR_ATTACK_FRAMES is the original's other
                    // trigger. A target switch or landing restarts the count.
                    if ctx.has_target && ctx.is_air_target {
                        if show.air_target != ctx.target_entity {
                            show.air_frames = 0;
                            show.air_target = ctx.target_entity;
                        }
                        show.air_frames += 1;
                        if show.air_frames >= AIR_ATTACK_FRAMES {
                            latch(&mut show, ctx);
                            start(&mut show);
                        }
                    } else {
                        show.air_frames = 0;
                        show.air_target = None;
                    }
                }
            }
            _ => {}
        }

        state.0 = serde_json::to_vec(&show).unwrap_or_default();
    }
}

const QUAKE_PROJECTILE: &str = "AAW";
const QUAKE_HITS_TO_SHOW: i32 = 2;
const AIR_ATTACK_FRAMES: i32 = 60;
const BLUE_INNER: [u8; 3] = [0, 35, 180];
const BLUE_SPREAD: [u8; 3] = [0, 20, 90];
const BEAM_HEIGHT: i32 = 50_000;
/// 256 leptons per cell, same as `Position::rx/ry` and `Coord2Cell`.
const LEPTONS_PER_CELL: i32 = 256;

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
    /// Quake shells that have gone off on the current target.
    quake_hits: i32,
    /// Target the quake count belongs to; a switch clears the count.
    quake_target: Option<u64>,
    /// Frames a flying target has been held (second trigger).
    air_frames: i32,
    air_target: Option<u64>,
    /// Latched target coords in leptons, with its `GetHeight()`: the ground
    /// under an air target sits `center.2 - height`.
    center: (i32, i32, i32),
    height: i32,
    air: bool,
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
            quake_target: None,
            air_frames: 0,
            air_target: None,
            center: (0, 0, 0),
            height: 0,
            air: false,
        }
    }
}

/// `center = target.GetCoords(); height = target.GetHeight()`.
fn latch(show: &mut Show, ctx: &ScriptContext<'_>) {
    show.center = ctx.anchor;
    show.height = ctx.target_height;
    show.air = ctx.is_air_target;
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
    show.air_frames = 0;
}

fn reset(show: &mut Show) {
    show.active = false;
    show.stage = 0;
    show.air_frames = 0;
}

/// `GameCreateLaser` with the script's `Thickness`. `house` selects
/// `IsHouseColor = true`: only the house-colour arm of the beam draw widens by
/// thickness, which is why the original's 3x3 release beams are a solid column
/// while the act-1 ring stays a hairline.
fn emit_beam(
    ctx: &mut ScriptContext<'_>,
    from: (i32, i32, i32),
    to: (i32, i32, i32),
    width: i32,
    duration: i32,
    house: bool,
) {
    let color = if house {
        LaserColor::House(ctx.owner)
    } else {
        LaserColor::Explicit {
            inner: BLUE_INNER,
            outer: BLUE_INNER,
            spread: BLUE_SPREAD,
        }
    };
    ctx.emit_laser(LaserBirth {
        frame: ctx.frame,
        from: ProjectileCoord::new(from.0, from.1, from.2),
        to: ProjectileCoord::new(to.0, to.1, to.2),
        z_adjust: 0,
        duration,
        width,
        supported: false,
        color,
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

    let (cx, cy, cz) = show.center;
    // GetCoords Z less GetHeight: the ground under an air target.
    let floor = cz - show.height;
    let mut index = 0usize;
    for step in 0..8 {
        let degrees = show.angle + step * 45;
        let radians = (degrees as f64).to_radians();
        let dx = (show.radius as f64 * radians.cos()) as i32;
        let dy = (show.radius as f64 * radians.sin()) as i32;
        if show.beams[index] {
            let x = cx + dx;
            let y = cy + dy;
            emit_beam(ctx, (x, y, cz + BEAM_HEIGHT), (x, y, floor), 4, 2, false);
            ctx.detonate(
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
        ctx.detonate("GBIONBeamLowWH", 100, (cx, cy, floor));
        if show.radius < 0 {
            show.stage = 2;
        }
    }
}

/// Act 2: charge, with the zero-damage charge warhead at frame 5.
fn charge(show: &mut Show, ctx: &mut ScriptContext<'_>) {
    if show.charge == 5 {
        let (x, y, z) = show.center;
        ctx.detonate("GBIONChargeWH", 0, (x, y, z - show.height));
    }
    show.charge += 1;
    if show.charge >= 20 {
        show.stage = 3;
    }
}

/// Act 3: lighting superweapon, a 3x3 bundle of thick beams, and the main wave.
fn release(show: &mut Show, ctx: &mut ScriptContext<'_>) {
    let (cx, cy, cz) = show.center;
    let floor = cz - show.height;
    // `beamCenter = Cell2Coord(Coord2Cell(center), center.Z)`: the beams and
    // the explosion anim both snap to the same cell, so they stay aligned.
    let cell_x = cx.div_euclid(LEPTONS_PER_CELL) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2;
    let cell_y = cy.div_euclid(LEPTONS_PER_CELL) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2;

    // The blue map lighting. The original launched a `Type=MultiMissile`
    // superweapon whose payload cannot spawn, so its only effect was the
    // `ScreenNukeFlash` with that type's `Light.*` tint; ask for it directly
    // instead of depending on a silo the player does not own.
    ctx.nuke_flash("GBIONBlueLight", 100);

    const OFFSETS: [i32; 3] = [-20, 0, 20];
    for ox in OFFSETS {
        for oy in OFFSETS {
            let x = cell_x + ox;
            let y = cell_y + oy;
            // `IsHouseColor = true` + `Thickness = 40`: the house-colour arm
            // is the only one that widens by thickness, so these 3x3 parallel
            // beams overlap into one solid column.
            // `LaserDuration=30` on the original `GBIONIonBigLaser`: the
            // column must outlive the single frame that created it.
            emit_beam(
                ctx,
                (x, y, cz + BEAM_HEIGHT),
                (x, y, floor),
                40,
                30,
                true,
            );
        }
    }

    if show.air {
        ctx.detonate("GBIONWaveAirWH", 700, (cell_x, cell_y, cz));
    }
    // NUKEANIM plus ShakeX/Y=20: the explosion animation, its Report sound
    // and the screen shake all come from this detonation.
    ctx.detonate("GBIONWaveWH", 700, (cell_x, cell_y, floor));

    show.stage = 4;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ruleset::RuleSet;
    use crate::sim::intern::InternedId;
    use crate::sim::scripting::{ScriptRegistry, ScriptStateStore, SimCommand};

    const SELF: u64 = 7;
    const TARGET: u64 = 9;
    const AIM: (i32, i32, i32) = (1000, 2000, 0);

    fn context<'a>(rules: &'a RuleSet, frame: i32) -> ScriptContext<'a> {
        let mut ctx = ScriptContext::new(rules, frame);
        ctx.has_target = true;
        ctx.target_entity = Some(TARGET);
        ctx.anchor = AIM;
        ctx
    }

    fn quake_hit(registry: &ScriptRegistry, states: &mut ScriptStateStore, frame: i32) {
        let rules = RuleSet::empty_for_test();
        let mut ctx = context(&rules, frame);
        ctx.projectile = Some("AAW");
        registry.dispatch_with(
            &ScriptEvent::BulletDetonated {
                owner: SELF,
                projectile: InternedId::default(),
                impact: (500, 600, 0),
            },
            states,
            ctx,
        );
    }

    #[test]
    fn two_quake_hits_start_the_show_at_the_target_not_the_caster() {
        let rules = RuleSet::empty_for_test();
        let mut registry = ScriptRegistry::new();
        registry.register(Box::new(GbionIonShow));
        let mut states = ScriptStateStore::default();

        // A held target alone never starts the show: the old fallback did, and
        // it detonated on the caster during construction.
        for frame in 0..600 {
            let out = registry.dispatch_with(
                &ScriptEvent::Update {
                    entity: SELF,
                    frame,
                },
                &mut states,
                context(&rules, frame),
            );
            assert!(
                out.commands.is_empty() && out.lasers.is_empty(),
                "no presentation without a quake hit"
            );
        }

        quake_hit(&registry, &mut states, 600);
        quake_hit(&registry, &mut states, 601);

        let mut saw_lasers = false;
        let mut saw_detonation = false;
        let mut saw_flash = false;
        for frame in 602..1000 {
            let out = registry.dispatch_with(
                &ScriptEvent::Update {
                    entity: SELF,
                    frame,
                },
                &mut states,
                context(&rules, frame),
            );
            saw_lasers |= !out.lasers.is_empty();
            for command in &out.commands {
                if let SimCommand::Detonate { coord, .. } = command {
                    // Never the caster's own coords (the latched target plus
                    // the ring offsets, never the entity's position).
                    assert!(
                        (coord.0 - AIM.0).abs() <= 200 && (coord.1 - AIM.1).abs() <= 200,
                        "detonation at {coord:?} is not on the target"
                    );
                    saw_detonation = true;
                }
                saw_flash |= matches!(command, SimCommand::NukeFlash { .. });
            }
        }
        assert!(saw_lasers, "act 1/3 must emit laser births");
        assert!(
            saw_detonation,
            "acts must detonate warheads so their AnimList and Report play"
        );
        assert!(saw_flash, "act 3 must start the blue map lighting");
        assert_eq!(registry.binding_for("gbionionshowscript"), Some(ScriptId(1)));
    }

    #[test]
    fn a_lone_quake_hit_without_a_target_anchors_on_the_impact() {
        let rules = RuleSet::empty_for_test();
        let mut registry = ScriptRegistry::new();
        registry.register(Box::new(GbionIonShow));
        let mut states = ScriptStateStore::default();

        for frame in 0..2 {
            let mut ctx = ScriptContext::new(&rules, frame);
            ctx.projectile = Some("AAW");
            registry.dispatch_with(
                &ScriptEvent::BulletDetonated {
                    owner: SELF,
                    projectile: InternedId::default(),
                    impact: (3000, 4000, 0),
                },
                &mut states,
                ctx,
            );
        }

        let mut first = None;
        for frame in 2..40 {
            let mut ctx = ScriptContext::new(&rules, frame);
            ctx.projectile = Some("AAW");
            let out = registry.dispatch_with(
                &ScriptEvent::Update {
                    entity: SELF,
                    frame,
                },
                &mut states,
                ctx,
            );
            if let Some(SimCommand::Detonate { coord, .. }) = out.commands.first() {
                first = Some(*coord);
            }
        }
        let coord = first.expect("act 1 must detonate");
        assert!(
            (coord.0 - 3000).abs() <= 200 && (coord.1 - 4000).abs() <= 200,
            "impact {coord:?} must anchor the show when no target is held"
        );
    }
}
