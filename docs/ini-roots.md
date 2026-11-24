# INI roots and archive names

A mod can name its Rules, Art, AI and catalogs in `mod.ini`. The game reads
those names through the existing loose-file and MIX loaders. Archive names
are separate: changing an INI name does not change which archives mount.
There are no built-in named-mod profiles or guesses based on a folder's name.

## A mod's files

Put `mod.ini` directly in the mod's folder and [select the mod](modding.md#pick-a-mod).
For example:

```ini
[Mod]
Name=Example Mod

[Files]
Rules=example-rules.ini
Art=example-art.ini
AI=example-ai.ini
Modes=example-modes.ini
MapCatalog=example-maps.pkt

[Archives]
ExpansionPrefix=expand_example
MapPrefix=maps_example
Multiplayer=example-multiplayer.mix
Theme=example-music.mix
MoviePrefix=movies_example

[Extensions]
Ares=yes
```

Each section is optional. A mod with no loading declarations can use its
conventional files as described below. With `[Files]`, omitted entries use the stock names
below, independent of the host's custom names. Rules, Art and AI must then
all resolve to nonempty, readable files before loading continues.

| `[Files]` key | Default | Read by |
| --- | --- | --- |
| `Rules` | `rulesmd.ini` | Ordered scenario Rules loading |
| `Art` | `artmd.ini` | Fixed Art definitions |
| `AI` | `aimd.ini` | AI definitions, extended by the map |
| `Sound` | `soundmd.ini` | Unlayered sound definitions |
| `Eva` | `evamd.ini` | Unlayered EVA definitions |
| `TheaterSuffix` | `md` | Theater control INIs, such as `snowmd.ini` |
| `Theme` | `thememd.ini` | Theme catalog |
| `Battle` | `BATTLEMD.INI` | Campaign catalog |
| `Mission` | `MISSIONMD.INI` | Mission metadata |
| `MapSelect` | `MAPSELMD.INI` | Campaign map-selection stages |
| `Modes` | `MPModesMD.ini` | Skirmish and network mode roster |
| `MapCatalog` | `missionsmd.pkt` | The initial multiplayer map catalog |

These are plain basenames: no directory paths or wildcards. `MapCatalog`
requires a `.pkt` name; the other file entries require `.ini`.
`TheaterSuffix` accepts only letters and digits. It replaces the `MD` in
theater control INI names, not the theater's MIX names. If that selected
theater INI is absent, the existing loader tries the stock name.

The catalog formats stay the same. `Modes` reads the native MPModes sections
and rows; `MapCatalog` reads a PKT's `[MultiMaps]` and per-map sections. Naming
a launcher's unrelated settings file does not convert its format. Loose
Skirmish checks loose PKT catalogs, YRO archives and multiplayer maps after
the initial catalog.

Optional catalogs keep their existing missing-file behavior. Sound and EVA
can produce empty catalogs; absent campaign or mission files do not become
stock campaign data. Selecting a different root does not add support for an
unimplemented gameplay key.

## Classic Red Alert 2 folder mods

For a selected mod without loading declarations or a CnCNet client catalog,
VERA looks for `rules.ini`, `art.ini`, `ai.ini` or `mpmodes.ini` in the
mod itself, including standard numbered `expandNN.mix` archives. It chooses
the RA2 family only when the package has no corresponding Yuri’s Revenge
roots or numbered YR expansion archives. An explicit custom INI profile
takes precedence.

In that family, `rules.ini`, `art.ini` and `ai.ini` resolve through the normal
mod and installation lookup. `mpmodes.ini` names the actual mode INIs, and
Skirmish uses `missions.pkt` and `multi.mix` for the classic map catalog.
Selecting **No mod** returns to the configured installation defaults.

## Archive names

`[Archives]` changes names in the existing startup slots:

| Key | Default | Files selected |
| --- | --- | --- |
| `ExpansionPrefix` | `expandmd` | Prefix plus `99.mix` down to `00.mix` |
| `MapPrefix` | `mapsmd` | Numbered map media, or the prefix's `*.mix` files in CD mode |
| `Multiplayer` | `multimd.mix` | Multiplayer archive |
| `Theme` | `thememd.mix` | Expansion theme archive |
| `MoviePrefix` | `movmd` | Numbered movie media, or the prefix's `*.mix` files in CD mode |

Prefixes accept ASCII letters, digits, `_` and `-`. The two filenames use
the same characters followed by `.mix`, and cannot name Windows devices.
Paths, empty names and wildcards are refused.

Mount order and fallback rules do not change. Expansions still mount in
descending numeric order; the native language, master, cache, local, audio,
conquer and cameo slots keep their names. Ares retains its own archive-order
rules. Numbered map and movie media keep their base `mapsNN.mix` and
`moviesNN.mix` fallbacks, and VERA still adds `theme.mix` after the selected
expansion theme archive. No directory-wide archive scan is added.

`[Extensions] Ares=yes` declares a requirement before startup reads any game
INI. With the player's Ares setting on Auto, it uses Ares reading for this
launch. Off refuses the mod and explains how to enable it. The saved player
setting is unchanged. See [Ares](ares.md#the-ares-setting). A declaration does
not load a DLL or supply missing files.
Ares is currently the supported declaration. Any other extension marked as
required is refused instead of being silently ignored.

The process asset owner freezes the chosen files and archive names. Recovery
uses that same choice rather than rereading a changed manifest or guessing
from newly available files. Restart after changing a mod's definition.

## Host configuration

Without a mod's `[Files]` declaration, `config.toml` can select INI roots:

```toml
[ini]
profile = "custom"
rules = "example-rules.ini"
art = "example-art.ini"
ai = "example-ai.ini"
sound = "example-sound.ini"
eva = "example-eva.ini"
theater_suffix = "example"
theme = "example-theme.ini"
battle = "example-battle.ini"
mission = "example-mission.ini"
mapsel = "example-mapselect.ini"
modes = "example-modes.ini"
map_catalog = "example-maps.pkt"
```

SOUNDMD and EVAMD take optional roots for any profile:

```toml
[ini]
sound = "my-sound.ini"
eva = "my-eva.ini"
```

Omitted names keep the profile's pair: `soundmd.ini`/`evamd.ini` for `yr` and
`custom`, `soundmo.ini`/`evamo.ini` for `mo` (and for `auto` once its rules
triplet selects MO). Both are unlayered presentation readers: never required,
never part of the preset completeness or ambiguity checks, and not part of the
multiplayer rules compatibility hash. A named file that is missing keeps the
audio owner's tolerated empty catalog instead of failing startup, exactly as
the fixed names did.

The theater INI takes an optional suffix for any profile:

```toml
[ini]
theater_suffix = "mo"
```

`Read_Theater_TileSets_INI` (`0x00545150`) names the file with `"%sMD.INI"`
from the theater's control name (`TEMPERAT`, `SNOW`, `URBAN`, `URBANN`,
`DESERT`, `LUNAR`). The suffix replaces `MD`: `md` for `yr` and `custom`, `mo`
for `mo` (and for `auto` once it selects MO), so a snow map reads
`snowmd.ini` or `snowmo.ini`. When the selected file is absent the loader warns
and reads the stock `snowmd.ini`, so a folder that loaded before keeps loading;
only when neither exists does that theater load fail. Theater MIX names
(`%sMD.MIX`) are unchanged. Letters and digits only; the suffix is not probed
at startup. Online, the rules agreement compares the theater INI each theater
reads, so players whose theater INIs differ are refused before a match, and
Quick Match names a theater INI that changes the retail one
([Networking](Networking.md)).

The profile also selects the theme and campaign files:

| Profile | Theme | Campaigns | Missions | Map select |
| --- | --- | --- | --- | --- |
| `yr`, `custom` | `thememd.ini` | `BATTLEMD.INI` | `MISSIONMD.INI` | `MAPSELMD.INI` |
| `mo` | `thememo.ini` | `BATTLEMO.INI` | `MISSIONMO.INI` | `MAPSELMO.INI` |

`auto` takes the MO names once it selects MO, though it keeps the YR archive
names ([Archive names](#archive-names)), so an MO folder whose files are packed
in its archives needs an explicit `mo`. gamemd reads the MD names
(`THEMEMD.INI`, `BATTLEMD.INI`, `MISSIONMD.INI`, `MAPSELMD.INI`); the MO names
are the ones Mental Omega's spawner gives them. They have no `[ini]` key and no
stock fallback, and they are not probed at startup. A missing file does what
gamemd does without it: no campaigns, no mission briefing or loading text, the
current map replayed at map select, or no YR theme catalog (VERA still adds
the RA2 `theme.ini` tracks).

Custom mode requires all three logical Rules/Art/AI basenames. Paths and
wildcards are rejected; these names are resolved through the existing VFS, not
arbitrary host file access. Overrides cannot be mixed with a preset. Explicit
`mo` and `custom` require all three nonempty readable winners before consumers
build data, and never fall back to YR when a selected root is missing. `yr`
preserves legacy admission: individual consumers still report unavailable files
at their original stage, without eagerly requiring gameplay INIs in asset-only
tools.

`auto` probes only files visible to the assembled resource provider. It accepts
YR 3/3 with MO 0/3, or MO 3/3 with YR 0/3. Both complete sets, a partial alternate
set, or no complete set are errors with counts and an explicit-selection hint.
This conservative policy avoids guessing from a directory name or silently
combining families. It detects root-file signatures, not a MOD release/version.

The asset manager freezes the resolved names before Rules/Art/AI consumers run.
Later theater or shell registrations do not rerun Auto. The process asset owner
retains that choice across a lost-manager reconstruction; it does not silently
change roots underneath an already constructed Rules registry. This is resource
configuration, not new gameplay state or a save-layout field.
Process-source diagnostics retain the selected logical Rules/Art filenames with
their frozen content hashes, rather than labelling MO/custom snapshots as
`RULESMD.INI` / `ARTMD.INI`. They do not reopen sources after scenario rebuilds.

### Archive names

Mental Omega's spawner renames gamemd's startup archive names before the game
opens them (`assets::asset_manager::ArchiveFamily`). An explicit
`profile = "mo"` mounts those names: `expandmo99.mix` down to `expandmo00.mix`
in place of the `expandmd` slots, then `mapsmo##.mix`, `multimo.mix`,
`thememo.mix` and `movmo##.mix` (`MAPSMO*.MIX` and `MOVMO*.MIX` in CD mode).
The master, cache, local, language, audio, conquer and cameo archives keep their
YR names. `yr`, `auto` and `custom` keep the YR names: Auto probes the roots
only after the archives are mounted, so it cannot choose the archives it
probes, and an MO folder needs an explicit `mo`. MO always plays its Ares
extension set: under `Ares=Auto`, the explicit `mo` profile reads Ares INI
syntax and Rules even when a pruned install has no `ares.mix`; `On` and `Off`
remain explicit overrides. Archive verbs of the `asset`
tool take `--ini-profile mo` for the same names and do not read the host
`[ini]`. The headless `headless_scenario::load_with_launch_and_ini` still
mounts the YR names.
Since v0.3.8 the startup archive names are resolved from the selected
folder's numbered archive family (`ArchiveNames`, see `assets::mod_resources`):
a folder shipping `expandmo##.mix` and `thememo.mix` gets the MO names
without a host `[ini]` key. Host `[ini]` settings still fix the Rules/Art/AI
root names; they never select an executable or another directory, and the
optional entries keep the same defaults and validation as `[Files]`.

## Inspection

Use the same definition with the asset tool:

```text
asset ini-roots --ra2-dir <game directory> --mod-dir <mod directory>
asset ini-get TEST Voxel --domain art --reader bool --default false --ra2-dir <game directory> --mod-dir <mod directory>
```

`--mod-dir` reads that folder's `mod.ini`, overlays its files and applies its
extension requirement. It does not use the desktop's selected mod.
Without it, INI commands use the host's `[ini]` settings unless
`--ini-profile yr|auto|custom` replaces them as a unit. A CLI custom profile
must supply `--rules-ini`, `--art-ini` and `--ai-ini` together.

`ini-roots --all-mixes` remains an explicit tooling-only wider search, not the
game's mounting policy. Source names and hashes describe consumed inputs;
they do not establish compatibility with a whole mod.

## Source and scope

baixiong443 adapted root selection from the ra2-mono files
`Ra2.Shared/GameConfig.cs`, `Ra2.IO/GameIniLoader.cs` and `Ra2.IO/IniMerger.cs`.
VERA keeps one root per family. The author contributed the adaptation under
VERA's GPLv3. [Original contribution](https://github.com/YuriPlanet/vera20k/commit/abe98da40b836f73de99902bc109f1aa43ce14af)

[Include and inheritance](ini-directives.md) run at the selected source
boundary when the game reads the [Ares way](ares.md), or in a `phobos` build.
Resource selection itself needs no extension build feature. Native archive
and layer ordering remain owned by the existing loaders.
