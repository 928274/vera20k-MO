#!/usr/bin/env bash
# Build the Mental Omega client the only correct way and deploy it.
#
# WHY THIS SCRIPT EXISTS (2026-11-10 regression):
#   A build was once produced from D:/WorkBuddy/Worktrees/vera20k/main-a66a04e3
#   (branch workbuddy/main-a66a04e3, 578 commits behind `mo`) with plain
#   `cargo build` and no feature flags. The result could not run the game:
#     * "CnCNet map loading failed: selected AI root aimo.ini failed structural
#       validation" — without `ares` there is no `+=` INI fallback, so MO's
#       TaskForces fragments never resolve.
#     * the loading screen painted the pre-game colour as Orange instead of the
#       chosen Blue — SkirmishColorMode::is_mo_color_table is
#       #[cfg(feature = "phobos")], so without `phobos` MO's `[Colors]` layout is
#       never recognised and the stock priority LUT answers entry 8 (Orange)
#       for id 6 (Blue).
#
# Source of truth: docs/MO-ADAPTATION.md, "构建与部署":
#   cargo build --release --no-default-features --features ares,phobos --bin vera20k
set -uo pipefail

SRC=/d/vera20k-mo                 # the real dev tree, branch `mo`
GAME="/d/GAMES/Mental Omega RA20K" # where the game + config.toml live
HERE="$(cd "$(dirname "$0")" && pwd)"

# shellcheck source=/dev/null
source "$HERE/win_env.sh"

fail() { printf '\n[build-mo] FAILED: %s\n' "$1" >&2; exit 1; }

# --- guards: refuse to build from the wrong tree or branch -------------------
[ -d "$SRC" ] || fail "$SRC does not exist"
git -C "$SRC" rev-parse --git-dir >/dev/null 2>&1 || fail "$SRC is not a git checkout"
cd "$SRC" || fail "cannot cd $SRC"
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = "mo" ] || fail "expected branch 'mo' in $SRC, found '$branch'"

# Other worktrees of this repo (D:/vera20k on `main`, and WorkBuddy's isolated
# D:/WorkBuddy/Worktrees/vera20k/main-a66a04e3) are NOT this fork's dev tree:
# they lack the MO adaptation commits, so never build the MO client from them.

printf '[build-mo] tree=%s branch=%s head=%s\n' "$SRC" "$branch" "$(git rev-parse --short HEAD)"

# --- build -------------------------------------------------------------------
before=$(stat -c %Y target/release/vera20k.exe 2>/dev/null || echo 0)
build_log=$(cargo build --release --no-default-features --features ares,phobos --bin vera20k 2>&1)
printf '%s\n' "$build_log" | tail -40
# A trailing `reg.exe` block from the sandbox surfaces as a non-zero status even
# though cargo already finished; judge by the artifact instead of the status.
# A stale artifact must never pass: on 2026-11-11 a failed compile (E0428) left
# the previous exe in place, the mtime comparison accepted it, and the script
# deployed the OLD binary as if it were the new build.
printf '%s' "$build_log" | grep -q 'could not compile' &&
    fail "cargo could not compile (see cargo output above)"
after=$(stat -c %Y target/release/vera20k.exe 2>/dev/null || echo 0)
[ "$after" -gt "$before" ] || fail "artifact not refreshed (see cargo output above)"

exe=target/release/vera20k.exe
[ -f "$exe" ] || fail "missing $exe"
size=$(stat -c %s "$exe")

# --- verify the mandatory capabilities are actually compiled in --------------
ares=$(grep -a -c 'Unknown Ares armor' "$exe" || true)
phobos=$(grep -a -c 'Phobos' "$exe" || true)
[ "$ares" -ge 1 ] || fail "ares is NOT compiled in (build the wrong binary?)"
[ "$phobos" -ge 5 ] || fail "phobos looks absent ($phobos markers)"
printf '[build-mo] artifact %s bytes, ares=%s phobos=%s\n' "$size" "$ares" "$phobos"

# --- deploy ------------------------------------------------------------------
[ -f "$GAME/vera20k.exe" ] && cp -f "$GAME/vera20k.exe" "$GAME/vera20k.prev.exe"
cp -f "$exe" "$GAME/vera20k.exe" || fail "copy into $GAME failed"
printf '[build-mo] deployed -> %s/vera20k.exe (previous kept as vera20k.prev.exe)\n' "$GAME"

# --- smoke test --------------------------------------------------------------
( cd "$GAME" && ./vera20k.exe -h >/dev/null 2>&1 )
printf '[build-mo] smoke test (-h) exit=%s\n' "$?"
printf '[build-mo] OK\n'
