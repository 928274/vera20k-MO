//! Unit tests of the rules set, one child module per subject; the shared
//! imports and the minimal rules fixture live here.

use super::*;
use crate::rules::native_processing::RulesLayerKind;

/// Build a minimal rules.ini string for testing.
fn make_test_rules() -> String {
    "\
[InfantryTypes]
0=E1
1=E2

[General]
BuildSpeed=0.75
MultipleFactory=0.7
LowPowerPenaltyModifier=1.25
MinLowPowerProductionSpeed=0.4
MaxLowPowerProductionSpeed=0.85

[VehicleTypes]
0=MTNK

[AircraftTypes]

[BuildingTypes]
0=GAPOWR

[E1]
Name=GI
Cost=200
Strength=125
Armor=flak
Speed=4
Primary=M60
BuildTimeMultiplier=1.15

[E2]
Name=Conscript
Cost=100
Strength=100
Armor=flak
Speed=4
Primary=INTL

[MTNK]
Name=Grizzly
Cost=700
Strength=300
Armor=heavy
Speed=6
Primary=105mm
Secondary=MachGun

[GAPOWR]
Name=Power Plant
Cost=800
Strength=750
Power=200
Foundation=2x2

[M60]
Damage=25
ROF=20
Range=5
Warhead=SA

[INTL]
Damage=20
ROF=20
Range=4.75
Warhead=SA

[105mm]
Damage=65
ROF=50
Range=5.75
Speed=40
Projectile=InvisibleLow
Warhead=AP
Burst=2

[MachGun]
Damage=20
ROF=15
Range=5
Projectile=InvisibleLow
Warhead=SA

[InvisibleLow]
AA=no
AG=yes

[SA]
Verses=100%,100%,100%,90%,70%,25%,100%,25%,25%,0%,0%
CellSpread=0

[AP]
Verses=100%,100%,90%,75%,75%,75%,60%,30%,20%,0%,0%
CellSpread=0
"
    .to_string()
}

#[test]
fn vera20k_ai_upgrade_keys_default_off_and_read_the_merged_ai_section() {
    use crate::rules::ini_parser::IniFile;

    let off = RuleSet::from_ini(&IniFile::from_str("[General]\n[AI]\n")).unwrap();
    assert!(!off.general.ai_aggressive_power_upgrades);
    assert!(!off.general.ai_fix_power_upgrades);

    let on = RuleSet::from_ini(&IniFile::from_str(
        "[General]\n[AI]\nVERA20K.AggressivePowerUpgrades=yes\n\
         VERA20K.FixPowerUpgrades=yes\n",
    ))
    .unwrap();
    assert!(on.general.ai_aggressive_power_upgrades);
    assert!(on.general.ai_fix_power_upgrades);

    let no = RuleSet::from_ini(&IniFile::from_str(
        "[General]\n[AI]\nVERA20K.AggressivePowerUpgrades=no\n\
         VERA20K.FixPowerUpgrades=no\n",
    ))
    .unwrap();
    assert!(!no.general.ai_aggressive_power_upgrades);
    assert!(!no.general.ai_fix_power_upgrades);
}

#[test]
fn ai_base_size_rules_read_the_merged_mo_ai_tail() {
    use crate::rules::ini_parser::IniFile;
    use crate::rules::ruleset::{AiBaseSizeRules, AiBuildCategory};

    let ini = IniFile::from_str(
        "[AI]\nBaseSizeAdd=24\nRefineryRatio=.08\nRefineryLimit=10\n\
         BarracksRatio=.08\nBarracksLimit=10\nWarRatio=.08\nWarLimit=10\n\
         DefenseRatio=.04\nDefenseLimit=80\nAARatio=.04\nAALimit=50\n\
         TeslaRatio=.04\nTeslaLimit=50\nHelipadRatio=.08\nHelipadLimit=5\n\
         AirstripRatio=.12\nAirstripLimit=0\nInfantryReserve=50000\n\
         InfantryBaseMult=5\nPowerSurplus=150\nPowerEmergency=75%\n\
         BuildRefinery=NAREFN,GAREFN\nBuildAA=NASAM,NAFLAK\n",
    );
    let ai = AiBaseSizeRules::from_ini_section(ini.section_or_empty("AI"));
    assert!(ai.enabled, "the MO [AI] tail enables the expansion rules");
    assert_eq!(ai.base_size_add, 24);
    assert_eq!(ai.infantry_reserve, 50_000);
    assert_eq!(ai.infantry_base_mult, 5);
    assert_eq!(ai.power_surplus, 150);
    assert!((ai.power_emergency - 0.75).abs() < 1e-6);
    assert!((ai.ratio(AiBuildCategory::Refinery) - 0.08).abs() < 1e-6);
    assert!((ai.ratio(AiBuildCategory::Defense) - 0.04).abs() < 1e-6);
    assert_eq!(ai.limit(AiBuildCategory::Refinery), 10);
    assert_eq!(ai.limit(AiBuildCategory::Defense), 80);
    assert_eq!(ai.limit(AiBuildCategory::Airstrip), 0);
    assert_eq!(
        ai.types(AiBuildCategory::Refinery)
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["NAREFN", "GAREFN"]
    );
    assert_eq!(
        ai.types(AiBuildCategory::Aa)
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["NASAM", "NAFLAK"]
    );
}

#[test]
fn ai_base_size_rules_stay_disabled_without_the_tail() {
    use crate::rules::ini_parser::IniFile;
    use crate::rules::ruleset::AiBaseSizeRules;

    let ini = IniFile::from_str("[AI]\nParanoid=yes\nCreditReserve=100\n");
    let ai = AiBaseSizeRules::from_ini_section(ini.section_or_empty("AI"));
    assert!(
        !ai.enabled,
        "an INI without the tail keeps the old behaviour"
    );
    assert_eq!(ai.base_size_add, 3);
    assert_eq!(ai.infantry_base_mult, 1);
    assert_eq!(ai.power_surplus, 50);
    assert!((ai.power_emergency - 0.75).abs() < 1e-6);
}

#[test]
fn ai_base_size_rules_survive_the_no_general_early_return() {
    use crate::rules::ini_parser::IniFile;
    use crate::rules::ruleset::AiBuildCategory;

    let general = GeneralRules::from_ini(&IniFile::from_str(
        "[AI]\nBaseSizeAdd=24\nRefineryLimit=10\nPowerSurplus=150\n",
    ));
    assert!(general.ai_base_size.enabled);
    assert_eq!(general.ai_base_size.base_size_add, 24);
    assert_eq!(general.ai_base_size.limit(AiBuildCategory::Refinery), 10);
    assert_eq!(general.ai_base_size.power_surplus, 150);
}

#[test]
fn ai_base_size_rules_read_the_last_include_layer() {
    use crate::rules::ini_parser::IniFile;
    use crate::rules::native_processing::{RulesLayerKind, RulesLayerStack};

    // MO: rulesmo.ini gives 6/50/1, then `[#include] +=ai.ini` gives
    // 24/150/5. The merged section is what the simulation must see.
    let mut layers = RulesLayerStack::new(IniFile::from_str(
        "[General]\n[AI]\nBaseSizeAdd=6\nRefineryLimit=10\nPowerSurplus=50\nInfantryBaseMult=1\n",
    ));
    layers.push(
        RulesLayerKind::Scenario,
        IniFile::from_str("[AI]\nBaseSizeAdd=24\nPowerSurplus=150\nInfantryBaseMult=5\n"),
    );
    let rules = RuleSet::from_rules_layers(&layers).unwrap();
    let ai = &rules.general.ai_base_size;
    assert!(ai.enabled);
    assert_eq!(ai.base_size_add, 24, "the later include wins over 6");
    assert_eq!(ai.power_surplus, 150);
    assert_eq!(ai.infantry_base_mult, 5);
}

mod config_hash_tests;
mod countries_tests;
mod general_tests;
mod types_tests;
