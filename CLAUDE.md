# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

SDLPoP is an open-source C port of the DOS game Prince of Persia, based on a disassembly of the original executable. The code structure deliberately mirrors the original segmented DOS memory model — function comments like `// seg000:024F` map back to disassembly offsets. This origin is important context: many seemingly odd patterns (global state in a single header, segment-named files) are inherited from the disassembly, not design choices.

## Building

Dependencies: `SDL2` and `SDL2_image` development libraries.

**Linux (make):**
```sh
cd src
make
# Binary is output to ../prince (project root)
```

**Linux (CMake with Ninja — preferred for speed):**
```sh
cd src
mkdir build && cd build
cmake -G Ninja ..
ninja
```

**Run:**
```sh
./prince                    # normal start
./prince megahit 3          # start at level 3 with cheats
./prince debug              # enable debug cheats
./prince mod "Mod Name"     # play a mod from mods/
```

There is no automated test suite.

## Architecture

All source is in `src/`. The codebase is pure C (C99), structured around the original DOS segments:

| File | Responsibility |
|------|---------------|
| `seg000.c` | Main loop (`pop_main`), game initialization, input, sound loading, sprite loading, HP display |
| `seg001.c` | Cutscene playback, sequence rendering for kid and opponent |
| `seg002.c` | Guard/shadow AI: initialization, HP, fallout checks, guard logic |
| `seg003.c` | Level initialization (`init_game`), the per-frame level loop (`play_level_2`), room redraw |
| `seg004.c` | Collision detection: wall/floor/ceiling checks, bump logic |
| `seg005.c` | Character movement: sequence table execution, falling, landing, control input, sword combat |
| `seg006.c` | Tile system: tile lookup, frame data, character position/direction helpers |
| `seg007.c` | Animated tiles ("trobs" = triggered objects): gates, spikes, loose floors, chompers |
| `seg008.c` | Room rendering: `draw_room`, `draw_tile`, wall drawing algorithm |
| `seg009.c` | Platform layer: SDL init/teardown, file I/O, path resolution, DAT file loading |
| `seqtbl.c` | Animation sequence bytecode table — defines every character animation as a byte stream |
| `options.c` | INI parser, `SDLPoP.ini` / `mod.ini` option loading, fixes/enhancements toggling |
| `replay.c` | Replay recording and playback (`.P1R` files) |
| `lighting.c` | Torch lighting and color palette effects |
| `screenshot.c` | Screenshot and level-map screenshot capture |
| `menu.c` | In-game pause menu |
| `midi.c` / `opl3.c` | MIDI playback via OPL3 emulation |

### Global state pattern

All game state variables are declared in `data.h` and defined in `data.c`. The trick: `data.h` uses `#ifdef BODY` — when included from `data.c` (which `#define BODY` first) it emits definitions with initializers; everywhere else it emits `extern` declarations. This means every `.c` file includes `common.h` → `data.h` and gets extern access to all globals.

### Header inclusion order

`common.h` is the single master include: it pulls in system headers, then `config.h`, `types.h`, `proto.h`, and `data.h` in that order. Every `.c` file starts with `#include "common.h"`.

### Compile-time feature flags

`config.h` controls features via `#define` / `#undef`: `USE_FADE`, `USE_FLASH`, `USE_COPYPROT`, `USE_QUICKSAVE`, `USE_REPLAY`, etc. These gates wrap optional game features.

### Fixes and enhancements system

Runtime bug fixes are controlled by the `fixes` pointer (set in `options.c`). When `use_fixes_and_enhancements` is true, `fixes` points to `fixes_saved` (user config); when false, it points to `fixes_disabled_state` (all off). Individual fixes are fields in this struct and are checked inline throughout the gameplay code.

## Configuration

- `SDLPoP.ini` — main config file in the project root (gameplay options, display, mods)
- `SDLPoP.cfg` — written by the in-game menu; overrides `.ini` until `.ini` is modified again
- `mods/<ModName>/mod.ini` — per-mod config that overrides gameplay options from `SDLPoP.ini`

## Data files

Game assets live in `data/`. `.DAT` files are the original DOS archive format. Music goes in `data/music/` as `.ogg` files (filenames listed in `data/music/names.txt`). Mods go in `mods/<ModName>/` and only need to include files that differ from the base game.
