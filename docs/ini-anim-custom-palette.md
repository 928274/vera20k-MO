# AnimType `CustomPalette=`

`CustomPalette=` is an **Ares 0.2** feature, not a stock Yuri's Revenge key.
The Ares documentation ("[Custom Animation and Projectile
Palettes](https://ares-developers.github.io/Ares-docs/new/customanimationandprojectilepalettes.html)")
states it plainly:

> In Yuri's Revenge, you could use the animation or the unit palette to
> display animations. Ares lets you to specify custom palettes for animations
> and projectiles.

An ART `[ANIM]` section can name its own palette file. The name is a *file*,
extension and all, and it is looked up without a theatre suffix by default.

```ini
[MCVMARK]                     ; an [Animations] type's ART section
CustomPalette=markpal.pal
```

The file replaces both the owner's selected ColorScheme and the global
`anim.pal` for every frame of that animation. A missing or unreadable file
leaves the ordinary arm in place rather than drawing nothing. An empty value is
an omitted key.

The animation is a plain convert: no ColorScheme is built, so the SHP's index
band 16..31 keeps the custom file's own colours instead of taking the owner's
house ramp. Mental Omega's `MCVMARK` — the marker over a deployable MCV —
depends on that, because its green corner lights are indices 16 and 22. Mental
Omega writes 541 `CustomPalette=` values, most of them `unittem.pal` or
`handshld.pal`.

## Theatre-specific palettes

Ares substitutes `~~~` with the active theatre's three-letter extension:
`CustomPalette=abcd.pal` always uses `abcd.pal`, while
`CustomPalette=lib~~~.pal` uses `libtem.pal`, `libsno.pal`, and so on. VERA
performs the substitution only under `--features ares`; a default build leaves
the literal name in place, so the load misses and the art falls back to its
ordinary palette.

## Projectiles

Ares applies the same convert to a BulletType's SHP frames, with one authoring
requirement:

> Note that if you're using this on projectiles, you have to add the projectile
> to the `[Animations]` list.

The BulletType's `Image=` names the AnimType whose ART section carries the
`CustomPalette=`. MO does this for the Seitaad's rising nanofibre needle:

```ini
; rulesmo.ini
[SeitaadMissileP]
Image=NANOBALLSM
; ... and 1321=NANOBALLSM in [Animations]

; artmo.ini
[NANOBALLSM]                  ; seitaad proj 1
CustomPalette=axespam.pal
```

`axespam.pal` (`expandmo96.mix`, 768 bytes) opens with a cyan/teal ramp
(`#4488C8`, `#408CBC`, `#38909C`, ...); `PALETTE.PAL` — `BulletClass::Draw`'s
ordinary global convert when neither `AnimPalette=` nor `FirersPalette=` is set
— opens with `#000000`, `#A800A8`, `#00A8A8`, `#00A800`, `#58FC54`, ...
Without the projectile arm the rising bullet was baked against the latter and
showed black/purple/green/yellow/red instead of cyan. Only the rise is a
Bullet; the burst uses `AnimList=NANOBALLSM2` and already took the animation
arm, which is why only that phase looked wrong.

## Where VERA20K owns it

- `rules/art_data.rs`: `AnimTypeRuntimeConfig::custom_palette` keeps the name
  verbatim (the native `ObjectTypeClass::ReadINI` 25-byte `CustomPalette`
  buffer reads the same key). `ares_custom_palette` is Ares'
  `AnimTypeExt+0xC` 2048-byte read (`1000A4B0`, `CustomPalette::LoadFromINI`);
  it is read but not consumed — the 25-byte field supplies the name.
- `render/sprite_atlas.rs`: `projectile_anim_palette_base` resolves a BulletType
  id to its `Image=` AnimType, the atlas decodes each named file once, and
  `sprite_palette_for_key` answers `SpritePaletteChoice::ExtraBuilding` for the
  animation contexts and (under `--features ares`) the three Bullet contexts.
  The house remap is skipped for that choice in every case.

## Residuals

- The hardware-cursor and placement-preview paths keep their own palette
  choices; neither draws an AnimType's `CustomPalette=`.
- A `CustomPalette=` on a type that also sets `Tiled=yes` yields the tiled
  `anim.pal` arm, as the native draw reads its literal first.
- MO's `[NANOBALLSM3]` writes `End=49` for a 25-frame SHP; VERA refuses to bind
  it (`invalid loaded End value 49 for 25 frames`). That is a separate MO ART
  data error affecting the muzzle flash, not the projectile's colours.
