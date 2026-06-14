---
name: feedback_sdlpop_replay_and_build
description: Corrections about SDLPoP build system and replay invocation — things I got wrong that caused wasted effort
metadata:
  type: feedback
---

**Read the README/docs before patching C source.** When stuck on a CLI invocation, grep the README first rather than making speculative code changes. Several unnecessary patches to seg000.c were made before finding the correct `validate` usage in the README.

**Why:** Patching unfamiliar C init/title-screen logic without understanding it caused cascading confusion and had to be reverted.

**How to apply:** For any SDLPoP CLI behaviour question, check README.md and grep seg000.c for `check_param` before touching source.

---

**Build from `src/build/`, not `src/`.**
```sh
cd src/build
cmake -G Ninja ..
ninja
```
CMakeLists.txt lives in `src/`, so the build dir must be inside `src/`.

**Why:** Running `cmake -G Ninja ..` from the project root fails because there's no CMakeLists.txt there.

---

**Replay invocation: `./prince validate replays/foo.p1r`**
- `validate` takes the path as the next positional arg (space-separated), not `validate=path`
- Do NOT pass a level number (e.g. `megahit 1`) alongside `validate` — it skips the title screen and breaks replay loading
- Do NOT use `seed=` alongside `validate` — the replay file must be `argv[1]` for seed to be honoured, which conflicts with the `validate` argv parsing

**Why:** These combinations were tried and failed before finding the correct form.

---

**The harness IS the test suite.** Don't write "there is no automated test suite" — `scripts/run_harness.sh` runs a deterministic replay and diffs against a golden trace. That is the test suite.
