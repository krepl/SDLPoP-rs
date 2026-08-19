# Native build — bug-hunt checklist

**For: Seth. Time: ~20 minutes. Why you and not automation:** every automated check in this
repo drives the game through `validate` mode or scripted input, and the browser side has now
been swept headlessly. The native build's *live* surface — a real SDL window, a real keyboard,
real timing — is the one place nothing has ever looked. Scripted input can prove the Kid moves;
it cannot tell us the game feels wrong.

```sh
cargo build && ./target/debug/prince          # normal start
./target/debug/prince megahit 3               # straight into level 3 with cheats
```

Note anything that looks off, however small. "Felt sluggish" is a useful report — I can chase
the cause. Don't try to diagnose; just say what you saw and roughly when.

## What to try

### Startup and shutdown
- [ ] Title screen appears; the demo/attract sequence plays if you wait
- [ ] Any key starts a game
- [ ] Quitting (Ctrl+Q, or the window close button) exits cleanly, no hang or crash

### Window and display
- [ ] Alt+Enter toggles fullscreen, both directions
- [ ] **Esc in fullscreen opens the pause menu** — this is the native counterpart to the
      browser bug just fixed. Native has no browser stealing Esc, so it *should* work; worth
      confirming the assumption.
- [ ] Resizing the window doesn't corrupt or stretch the picture oddly
- [ ] Nothing flickers or tears during normal play

### Pause menu
- [ ] Esc opens it, Esc closes it, game resumes where it left off
- [ ] Backspace also opens it
- [ ] Arrow keys + Enter navigate; Settings → each submenu (General/Gameplay/Visuals/Mods/Controls)
- [ ] Changing a setting takes effect, and survives a restart of the game
- [ ] Mouse works in the menu: hovering highlights, clicking selects
- [ ] "Restart Level", "Restart Game", "Quit Game" each do what they say

### Save/load
- [ ] F6 quicksave, F9 quickload — Kid returns to the saved spot
- [ ] Quickload after quitting and relaunching still works
- [ ] Ctrl+G save / Ctrl+L load (**levels 3–13 only** — doing nothing on level 1 is correct)
- [ ] Dying and continuing restores sensibly

### Playing
- [ ] Movement feels right: run, stop, turn, crouch, climb, jump — no stickiness or lag
- [ ] Shift: pick up potion / sword, careful step, ledge grab
- [ ] Sword fighting: draw, strike, parry, retreat
- [ ] Falling, spikes, chompers, loose floors, gates, pressure plates
- [ ] Level transitions
- [ ] Cutscenes play and are skippable

### Audio
- [ ] Music and sound effects play; no stutter, crackle or drift out of sync
- [ ] **Known environment issue:** your WSLg audio has been broken before. If there's no sound
      at all, that's probably the environment, not the port — try
      `SDL_AUDIODRIVER=dummy ./target/debug/prince` to rule audio out and keep testing.

### Odds and ends
- [ ] Space shows time remaining
- [ ] Backquote (`) fast-forwards
- [ ] F12 screenshot; Shift+F12 level map
- [ ] Ctrl+A restart level
- [ ] Hall of fame appears after finishing/dying appropriately, and persists

## Reporting

Anything at all: what you did, what happened, what you expected. Rough timing is enough. If
something is reproducible, the input sequence is gold — I can usually turn it into a scripted
scenario under `scripts/scripted_inputs/` and then it's a permanent regression test, checked
against the C oracle on every CI run.
