# Plan 14 — Fixing original-game bugs without losing C parity

**Status:** design agreed, implementation deferred until there's a real bug list worth fixing.

## Context

The port's prime directive is *no behaviour changes* — quirks may be load-bearing, and the
whole test apparatus (30 replay traces, pixel hashes, `live_surface_diff.sh`) is built on
matching the C oracle exactly. That directive is about the *port*, though, not about the
project forever. Once the port is trustworthy, the point is to improve the game.

So: how do we fix bugs in a 1989 game without destroying the thing that proves our port is
faithful?

## The mechanism already exists

Upstream SDLPoP solved this and we inherited it. `fixes_options_type` (`c/types.h:1211`)
already carries **43 flags** — `fix_two_coll_bug`, `fix_gate_sounds`,
`enable_crouch_after_climbing`, and so on. They are:

- consulted inline at the call site: `if (*fixes).fix_two_coll_bug == 0 { return; }`
  (`rust/src/seg004.rs:271`)
- configurable per-user from `SDLPoP.ini`, and per-mod from `mod.ini`
- switchable wholesale: `use_fixes_and_enhancements` swings the `fixes` pointer between
  `fixes_saved` (user config) and `fixes_disabled_state` (everything off)

**We should extend this, not invent anything.** A new fix is a new field, defaulting off.

## Why this preserves parity for free

With a flag off, the code path is identical to the original, so:

- all 30 replays and their golden traces keep passing, untouched
- `cargo xtask live-diff` keeps passing, because both builds take the original branch
- a replay recorded with a fix on records that fact, so it still replays correctly

Parity stops being "we never changed anything" and becomes the stronger, checkable claim:
**with fixes off, we are byte-identical to the original.**

## The part that is genuinely hard

**A fix cannot be validated by the oracle.** The C oracle is the definition of original
behaviour; asking it whether our fix is *correct* is meaningless. It can only confirm the fix
is *inert when off*. So every fix needs two different tests:

| Flag state | What it proves | Cost |
|---|---|---|
| **off** | byte-identical to the original | free — the existing harness already does this |
| **on** | the new behaviour is what we intended | new work, per fix, hand-written |

This is the real price of fixing bugs, and it is worth stating up front: the safety net that
makes this project pleasant only covers half of it.

## Where a fix should live

Two flavours, and picking the right one per fix matters:

**1. Mirror into both `c/` and `rust/` (default for anything touching simulation state).**
The C oracle stays authoritative for both flag states, so `live-diff` can validate the
fixed path too, not just the original one. Costs writing the fix twice — but the C side is
the reference implementation, which is the discipline this project already runs on.

**2. Rust-only, or platform-layer-only (for anything that cannot affect the trace).**
Presentation and platform concerns — input mapping, browser affordances — never reach the
state trace, so mirroring buys nothing. This is exactly what the fullscreen pause fix did
(`b0802fc`): `KeyP` is aliased to Escape's scancode in `web/index.html`, so the engine only
ever sees a scancode it already binds, and parity is untouched by construction.

Rule of thumb: **if it can change a trace field or a pixel, mirror it into C.**

## Candidate bugs

Deliberately short, because we do not have a real list yet. Everything currently known is
either faithful-and-harmless or not a bug at all — see `docs/deferred-work.md`.

| Candidate | Severity | Notes |
|---|---|---|
| Kid not redrawn after quickload until input | Low | Cosmetic, self-corrects on any keypress. Confirmed faithful to C. The obvious first candidate precisely *because* it is low-stakes — a good one to prove the mechanism on. |
| Sub-12ms keypresses dropped (wasm) | Low | Platform-layer, so flavour 2. Not a game bug. |
| Whatever the native bug-hunt turns up | Unknown | `docs/native-bug-hunt-checklist.md` is the input to this. |

**Recommendation: defer implementation until the native pass produces a real list.** Building
the mechanism now would be building it for one cosmetic bug. The mechanism is already there;
what's missing is bugs worth spending it on.

## When we do start

1. Add the field to `fixes_options_type` in `c/types.h`, defaulting off, and to the INI
   parser in `options.c` / `rust/src/options.rs` (follow any existing `fix_*` as a template).
2. Guard the fix at the call site: `if (*fixes).fix_foo != 0 { new } else { original }`.
3. Mirror into the other build if it can affect the trace.
4. `cargo xtask verify` — proves the flag is inert when off.
5. Write the flag-on regression test. This is the part with no safety net; it needs care.
6. Record the fix in `SDLPoP.ini` with a comment explaining the original behaviour, so anyone
   wanting authenticity can turn it back off.
