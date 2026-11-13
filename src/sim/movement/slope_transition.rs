//! Drive/Ship locomotor slope cache and global-frame transition timer.
//!
//! Active `gamemd.exe` stores this state on each Drive/Ship locomotor object:
//! Drive constructor `0x004AF540`, Process `0x004B0500`, force helper
//! `0x004AFB40`, and Draw_Matrix `0x004AFF60`; Ship uses the instruction-for-
//! instruction equivalent constructor `0x0069EC50`, Process `0x0069FC10`,
//! force helper `0x0069F250`, and Draw_Matrix `0x0069F670`. The native timer
//! helpers are `CDTimerClass::Start @ 0x0046B640` and `Remaining @ 0x004B4D70`.

use crate::map::entities::EntityCategory;
use crate::sim::game_entity::GameEntity;
use crate::sim::timer::CdTimer;

/// Literal Drive/Ship slope interpolation duration installed by both native
/// `Process` implementations.
pub const SLOPE_TRANSITION_FRAMES: u8 = 3;

/// Defined, persistent state owned by one active or stashed Drive/Ship
/// locomotor. The native unused timer dword is deliberately not represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SlopeTransitionState {
    previous_slope: u8,
    current_slope: u8,
    start_frame: i32,
    transition_total: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlopeRenderPhase {
    Stable(u8),
    Transition {
        from_slope: u8,
        to_slope: u8,
        phase_num: i32,
        phase_den: u8,
    },
}

impl SlopeTransitionState {
    /// Drive/Ship constructor defaults use the current signed global frame.
    pub(crate) fn at_binary_frame(binary_frame: u32) -> Self {
        Self {
            previous_slope: 0,
            current_slope: 0,
            start_frame: binary_frame as i32,
            transition_total: 0,
        }
    }

    /// `Force_Slope`/successful Foot unlimbo synchronizes immediately.
    pub fn snap(&mut self, sampled_slope: u8, binary_frame: u32) {
        self.previous_slope = sampled_slope;
        self.current_slope = sampled_slope;
        self.start_frame = binary_frame as i32;
        self.transition_total = 0;
    }

    /// Drive/Ship `Process` entry starts a literal three-frame timer only when
    /// the current containing-cell slope differs from the cached target.
    pub fn sample_process_entry(&mut self, sampled_slope: u8, binary_frame: u32) {
        self.sample_process_entry_with(sampled_slope, binary_frame, false);
    }

    /// [`Self::sample_process_entry`] for `VERA20K.SmoothSlopeTransition=`.
    ///
    /// A vehicle crossing cell boundaries usually meets the next cell's slope
    /// before the three-frame timer has run out. The original write
    /// `previous = current; current = sampled; timer = 3` then snaps the body
    /// back to the *previous target's* full tilt for the next frame, because
    /// `render_phase` answers `blend(previous, current, 0/3)`, while the frame
    /// before it drew `blend(previous_old, current_old, 1/3..2/3)`. On a
    /// sequence of differing slopes that reads as the hull pumping up and down.
    /// Retargeting instead keeps the origin and the running timer, so the tilt
    /// keeps advancing from the pose already on screen and still lands on the
    /// new slope when the timer expires. A retarget back onto the origin is
    /// still a normal restart: the two table entries must differ for a blend
    /// to mean anything.
    pub fn sample_process_entry_smooth(&mut self, sampled_slope: u8, binary_frame: u32) {
        self.sample_process_entry_with(sampled_slope, binary_frame, true);
    }

    fn sample_process_entry_with(
        &mut self,
        sampled_slope: u8,
        binary_frame: u32,
        smooth_retarget: bool,
    ) {
        if sampled_slope == self.current_slope {
            return;
        }
        if smooth_retarget
            && self.transition_total != 0
            && self.remaining(binary_frame) > 0
            && self.previous_slope != sampled_slope
        {
            self.current_slope = sampled_slope;
            return;
        }
        self.previous_slope = self.current_slope;
        self.current_slope = sampled_slope;
        self.start_frame = binary_frame as i32;
        self.transition_total = SLOPE_TRANSITION_FRAMES;
    }

    /// The timer's time left. Native starts it (`+0x24`/`+0x2C`) with the
    /// same value it writes to the total (`+0x30`), so the total stands for
    /// both.
    pub(crate) fn remaining(&self, binary_frame: u32) -> i32 {
        CdTimer::from_raw(self.start_frame, i32::from(self.transition_total))
            .remaining(binary_frame as i32)
    }

    pub fn render_phase(&self, binary_frame: u32) -> SlopeRenderPhase {
        let remaining = self.remaining(binary_frame);
        if self.transition_total == 0 || remaining == 0 {
            return SlopeRenderPhase::Stable(self.current_slope);
        }
        let phase_num = i32::from(self.transition_total).wrapping_sub(remaining);
        if phase_num >= i32::from(self.transition_total) {
            SlopeRenderPhase::Stable(self.current_slope)
        } else {
            SlopeRenderPhase::Transition {
                from_slope: self.previous_slope,
                to_slope: self.current_slope,
                phase_num,
                phase_den: self.transition_total,
            }
        }
    }

    pub(crate) fn hash_fields(&self) -> (u8, u8, i32, u8) {
        (
            self.previous_slope,
            self.current_slope,
            self.start_frame,
            self.transition_total,
        )
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn from_fields_for_test(
        previous_slope: u8,
        current_slope: u8,
        start_frame: i32,
        transition_total: u8,
    ) -> Self {
        Self {
            previous_slope,
            current_slope,
            start_frame,
            transition_total,
        }
    }
}

fn foot_equivalent(category: EntityCategory) -> bool {
    matches!(
        category,
        EntityCategory::Unit | EntityCategory::Infantry | EntityCategory::Aircraft
    )
}

/// The active Drive/Ship locomotor's slope state, as `DriveLocomotionClass::Draw_Matrix`
/// (`0x004AFF60`) and `ShipLocomotionClass::Draw_Matrix` (`0x0069F670`) read it.
pub fn state_for_entity(entity: &GameEntity) -> Option<&SlopeTransitionState> {
    foot_equivalent(entity.category())
        .then_some(())
        .and_then(|()| entity.locomotor.as_ref()?.active_slope_transition())
}

/// The same state for its writers: the Process samples (Drive `0x004B050B`, Ship
/// `0x0069FC1B`) and `FootClass::Unlimbo`'s force-slope slot (`0x004D71A9`).
fn state_for_entity_mut(entity: &mut GameEntity) -> Option<&mut SlopeTransitionState> {
    foot_equivalent(entity.category())
        .then_some(())
        .and_then(|()| entity.locomotor.as_mut()?.active_slope_transition_mut())
}

/// FootClass::Unlimbo @ `0x004D7170` dispatches the active locomotor force-
/// slope slot at `0x004D71A9` only after TechnoClass::Unlimbo succeeds.
pub(crate) fn snap_after_successful_unlimbo(
    entity: &mut GameEntity,
    sampled_slope: u8,
    binary_frame: u32,
) {
    if let Some(state) = state_for_entity_mut(entity) {
        state.snap(sampled_slope, binary_frame);
    }
}

/// Whether [`sample_process_entry`] changes `entity`'s slope state for
/// `sampled_slope`: it has one, and the slope differs from its target.
pub(crate) fn process_entry_changes(entity: &GameEntity, sampled_slope: u8) -> bool {
    state_for_entity(entity).is_some_and(|state| state.current_slope != sampled_slope)
}

/// Drive Process `0x004B050B..0x004B0557` and Ship Process
/// `0x0069FC1B..0x0069FC67` sample before any track/movement branch.
/// `smooth_retarget` is `VERA20K.SmoothSlopeTransition=`, which only changes
/// what a sample landing inside a running transition does.
pub(crate) fn sample_process_entry(
    entity: &mut GameEntity,
    sampled_slope: u8,
    binary_frame: u32,
    smooth_retarget: bool,
) {
    if let Some(state) = state_for_entity_mut(entity) {
        if smooth_retarget {
            state.sample_process_entry_smooth(sampled_slope, binary_frame);
        } else {
            state.sample_process_entry(sampled_slope, binary_frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SlopeRenderPhase, SlopeTransitionState};
    use crate::map::entities::EntityCategory;
    use crate::rules::locomotor_type::LocomotorKind;
    use crate::sim::game_entity::GameEntity;
    use crate::sim::movement::locomotion::LocomotorRuntimePayload;
    use crate::sim::movement::locomotor::LocomotorState;

    #[test]
    fn slope_timer_preserves_signed_minus_one_and_negative_phase() {
        assert_eq!(super::SLOPE_TRANSITION_FRAMES, 3);
        let mut state = SlopeTransitionState::at_binary_frame(0);
        state.previous_slope = 4;
        state.current_slope = 9;
        state.start_frame = 0;
        state.transition_total = super::SLOPE_TRANSITION_FRAMES;

        assert_eq!(
            state.render_phase(u32::MAX),
            SlopeRenderPhase::Transition {
                from_slope: 4,
                to_slope: 9,
                phase_num: -1,
                phase_den: super::SLOPE_TRANSITION_FRAMES,
            }
        );
        assert_eq!(state.remaining(0xffff_feff), 260);
        assert_eq!(
            state.render_phase(0xffff_feff),
            SlopeRenderPhase::Transition {
                from_slope: 4,
                to_slope: 9,
                phase_num: -257,
                phase_den: super::SLOPE_TRANSITION_FRAMES,
            }
        );

        state.start_frame = -1;
        assert_eq!(state.remaining(0), 3);
        assert_eq!(state.remaining(u32::MAX), 3);
    }

    #[test]
    fn slope_timer_uses_signed_wrapping_frame_subtraction() {
        let mut state = SlopeTransitionState::at_binary_frame(0x7fff_fffe);
        state.sample_process_entry(7, 0x7fff_ffff);
        assert_eq!(state.remaining(0x8000_0000), 2);
        assert_eq!(state.remaining(0x8000_0001), 1);
        assert_eq!(state.remaining(0x8000_0002), 0);
    }

    #[test]
    fn equal_process_sample_is_a_complete_no_write() {
        let mut state = SlopeTransitionState::at_binary_frame(10);
        state.sample_process_entry(6, 10);
        let before = state;
        state.sample_process_entry(6, 99);
        assert_eq!(state, before);
    }

    #[test]
    fn drive_ship_slope_three_frame_ledger_and_mid_transition_retarget_are_exact() {
        let mut state = SlopeTransitionState::at_binary_frame(8);
        state.snap(2, 8);
        state.sample_process_entry(7, 10);
        assert_eq!(
            state.render_phase(10),
            SlopeRenderPhase::Transition {
                from_slope: 2,
                to_slope: 7,
                phase_num: 0,
                phase_den: super::SLOPE_TRANSITION_FRAMES,
            }
        );
        assert_eq!(state.remaining(11), 2);
        assert_eq!(state.remaining(12), 1);
        assert_eq!(state.render_phase(13), SlopeRenderPhase::Stable(7));

        state.sample_process_entry(12, 11);
        assert_eq!(state.hash_fields(), (7, 12, 11, 3));
        assert_eq!(
            state.render_phase(11),
            SlopeRenderPhase::Transition {
                from_slope: 7,
                to_slope: 12,
                phase_num: 0,
                phase_den: super::SLOPE_TRANSITION_FRAMES,
            }
        );
    }

    /// The original `Process` write snaps a body back to the previous target's
    /// full tilt whenever the containing cell changes slope while the
    /// three-frame timer still runs, because it re-bases `previous` on the old
    /// target and zeroes the phase. Retargeting keeps both the origin and the
    /// timer, so the tilt continues from the pose already on screen.
    #[test]
    fn smooth_retarget_keeps_the_running_transition_instead_of_snapping_to_the_old_target() {
        let phase = |state: &SlopeTransitionState, frame: u32| state.render_phase(frame);

        let mut native = SlopeTransitionState::at_binary_frame(0);
        native.snap(0, 0);
        native.sample_process_entry(1, 2);
        assert_eq!(
            phase(&native, 2),
            SlopeRenderPhase::Transition {
                from_slope: 0,
                to_slope: 1,
                phase_num: 0,
                phase_den: 3
            }
        );
        assert_eq!(
            phase(&native, 3),
            SlopeRenderPhase::Transition {
                from_slope: 0,
                to_slope: 1,
                phase_num: 1,
                phase_den: 3
            }
        );
        native.sample_process_entry(2, 4);
        assert_eq!(
            phase(&native, 4),
            SlopeRenderPhase::Transition {
                from_slope: 1,
                to_slope: 2,
                phase_num: 0,
                phase_den: 3
            },
            "the original write re-bases from_slope on the old target and restarts the phase"
        );

        let mut smooth = SlopeTransitionState::at_binary_frame(0);
        smooth.snap(0, 0);
        smooth.sample_process_entry_smooth(1, 2);
        assert_eq!(
            phase(&smooth, 3),
            SlopeRenderPhase::Transition {
                from_slope: 0,
                to_slope: 1,
                phase_num: 1,
                phase_den: 3
            }
        );
        smooth.sample_process_entry_smooth(2, 4);
        assert_eq!(smooth.hash_fields(), (0, 2, 2, 3));
        assert_eq!(
            phase(&smooth, 4),
            SlopeRenderPhase::Transition {
                from_slope: 0,
                to_slope: 2,
                phase_num: 2,
                phase_den: 3
            },
            "the origin and the running timer survive, so the tilt keeps converging"
        );
        assert_eq!(phase(&smooth, 5), SlopeRenderPhase::Stable(2));

        // A sample back onto the origin still restarts: a blend needs two
        // different table entries.
        let mut back = SlopeTransitionState::at_binary_frame(0);
        back.snap(0, 0);
        back.sample_process_entry_smooth(1, 2);
        back.sample_process_entry_smooth(0, 3);
        assert_eq!(back.hash_fields(), (1, 0, 3, 3));
    }

    fn entity_with(category: EntityCategory, kind: LocomotorKind) -> GameEntity {
        let mut entity = GameEntity::test_default(1, "SLOPE", "Americans", 2, 2);
        entity.set_category_for_test(category);
        entity.locomotor = Some(LocomotorState::for_test_kind(kind));
        entity
    }

    #[test]
    fn slope_ownership_is_foot_plus_matching_active_drive_ship_only() {
        for category in [
            EntityCategory::Unit,
            EntityCategory::Infantry,
            EntityCategory::Aircraft,
        ] {
            assert!(
                super::state_for_entity(&entity_with(category, LocomotorKind::Drive)).is_some()
            );
            assert!(super::state_for_entity(&entity_with(category, LocomotorKind::Ship)).is_some());
        }
        assert!(
            super::state_for_entity(&entity_with(
                EntityCategory::Structure,
                LocomotorKind::Drive
            ))
            .is_none()
        );
        for kind in [
            LocomotorKind::Walk,
            LocomotorKind::Hover,
            LocomotorKind::Fly,
            LocomotorKind::Jumpjet,
            LocomotorKind::Rocket,
            LocomotorKind::Teleport,
        ] {
            assert!(super::state_for_entity(&entity_with(EntityCategory::Unit, kind)).is_none());
        }

        let mut mismatched = entity_with(EntityCategory::Unit, LocomotorKind::Drive);
        mismatched
            .locomotor
            .as_mut()
            .unwrap()
            .set_runtime_payload_for_test(LocomotorRuntimePayload::Walk(Default::default()));
        assert!(super::state_for_entity(&mismatched).is_none());
    }

    #[test]
    fn active_drive_piggyback_is_eligible_while_stashed_drive_is_not_active() {
        let constructor = LocomotorState::for_test_kind_at_frame(LocomotorKind::Drive, 17);
        assert_eq!(
            constructor.active_slope_transition().unwrap().hash_fields(),
            (0, 0, 17, 0),
            "ordinary construction retains the live nonzero frame"
        );

        let mut active_drive = entity_with(EntityCategory::Unit, LocomotorKind::Teleport);
        assert!(
            active_drive
                .locomotor
                .as_mut()
                .unwrap()
                .begin_piggyback(LocomotorKind::Drive, 22)
        );
        assert!(super::state_for_entity(&active_drive).is_some());
        assert_eq!(
            super::state_for_entity(&active_drive)
                .unwrap()
                .hash_fields(),
            (0, 0, 22, 0),
            "fresh Drive replacement retains the live nonzero frame"
        );
        assert_eq!(
            active_drive.locomotor.as_ref().unwrap().effective_kind(),
            LocomotorKind::Teleport
        );

        let mut stashed_drive = entity_with(EntityCategory::Unit, LocomotorKind::Drive);
        assert!(
            stashed_drive
                .locomotor
                .as_mut()
                .unwrap()
                .begin_piggyback(LocomotorKind::Teleport, 23)
        );
        assert!(super::state_for_entity(&stashed_drive).is_none());
        assert!(matches!(
            stashed_drive
                .locomotor
                .as_ref()
                .unwrap()
                .piggyback()
                .as_deref()
                .map(|stashed| stashed.runtime_payload()),
            Some(LocomotorRuntimePayload::Drive(_))
        ));
        assert!(
            stashed_drive
                .locomotor
                .as_mut()
                .unwrap()
                .end_piggyback()
                .is_some()
        );
        assert_eq!(
            super::state_for_entity(&stashed_drive)
                .unwrap()
                .hash_fields(),
            (0, 0, 0, 0),
            "generic piggyback restore does not invent a terrain snap"
        );
    }
}
