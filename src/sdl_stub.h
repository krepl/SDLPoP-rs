/*
Minimal stand-in for <SDL2/SDL.h>/<SDL2/SDL_image.h>, used ONLY when bindgen parses
common.h for the wasm32-unknown-unknown target (see types.h's SDLPOP_BINDGEN_WASM_STUB
guard and build.rs). The wasm build never links against real SDL2 -- WasmRenderer
(rust/src/platform/wasm.rs) is a from-scratch reimplementation -- so real SDL2 dev headers
have no runtime purpose there; they were only needed because bindgen has to parse this same
shared header chain for both targets, and common.h unconditionally pulled in real
<SDL2/SDL.h>. This file exists so a bare-metal wasm-only build/deploy environment (a
server/droplet that only ever serves the browser build, per the user's explicit goal) never
needs libsdl2-dev/libsdl2-image-dev installed at all.

Scope: everything the rest of the header chain (types.h/data.h/proto.h) actually needs
bindgen to see when parsing for wasm32. Two categories:

1. Opaque handle types (SDL_Surface, SDL_Window, ...) -- always threaded through the
   codebase as raw pointers (`*mut SDL_Surface` etc.), never dereferenced by value in any
   code that compiles for wasm32 (native-only field access lives in platform/sdl.rs, gated
   `#[cfg(not(target_arch = "wasm32"))]`, so never reaches this target at all). An
   incomplete/opaque C type is all bindgen needs to produce a matching opaque Rust type for
   these.

2. Real field-complete struct layouts (SDL_Rect, SDL_Color, SDL_Palette, SDL_PixelFormat)
   -- confirmed via a full audit (grep for `SDL_Foo {` struct-literal construction across
   rust/src) that Rust code on BOTH platforms constructs/reads these by value with named
   fields (rust/src/{seg000,seg008,seg009,lighting,menu,screenshot}.rs,
   platform/{mod,wasm}.rs) -- so wasm32 needs the exact same field shape bindgen would
   generate from the real SDL2 headers on native. Copied field-for-field from SDL2's real
   SDL_rect.h/SDL_pixels.h (verified against the installed libsdl2-dev headers, SDL2 2.32).
   If a future change starts constructing some *other* SDL_* type by value anywhere in
   cross-platform code, it needs adding here too -- this is not automatically kept in sync,
   the way parsing the real header was.
*/
#ifndef SDLPOP_SDL_STUB_H
#define SDLPOP_SDL_STUB_H

#include <stdint.h>

typedef int8_t   Sint8;
typedef uint8_t  Uint8;
typedef int16_t  Sint16;
typedef uint16_t Uint16;
typedef int32_t  Sint32;
typedef uint32_t Uint32;
typedef int64_t  Sint64;
typedef uint64_t Uint64;

#define SDL_LIL_ENDIAN 1234
#define SDL_BIG_ENDIAN 4321
#define SDL_BYTEORDER SDL_LIL_ENDIAN

/* Real SDL2's own definition (SDL_stdinc.h) for the pre-C11/no-__COUNTER__ fallback path --
   types.h calls this for real (SDL_COMPILE_TIME_ASSERT(level_size, ...) etc.), so it must
   actually work, not just be present. */
#define SDL_COMPILE_TIME_ASSERT(name, x) \
    typedef int SDL_dummy_ ## name[(x) * 2 - 1]

typedef struct SDL_Surface SDL_Surface;
typedef struct SDL_Window SDL_Window;
typedef struct SDL_Renderer SDL_Renderer;
typedef struct SDL_Texture SDL_Texture;
typedef struct SDL_RWops SDL_RWops;
typedef struct SDL_GameController SDL_GameController;
typedef struct SDL_Joystick SDL_Joystick;
typedef struct SDL_Haptic SDL_Haptic;

typedef struct SDL_Rect {
    int x, y;
    int w, h;
} SDL_Rect;

typedef struct SDL_Color {
    Uint8 r, g, b, a;
} SDL_Color;

typedef struct SDL_Palette {
    int ncolors;
    SDL_Color *colors;
    Uint32 version;
    int refcount;
} SDL_Palette;

typedef struct SDL_PixelFormat {
    Uint32 format;
    SDL_Palette *palette;
    Uint8 BitsPerPixel;
    Uint8 BytesPerPixel;
    Uint8 padding[2];
    Uint32 Rmask, Gmask, Bmask, Amask;
    Uint8 Rloss, Gloss, Bloss, Aloss;
    Uint8 Rshift, Gshift, Bshift, Ashift;
    int refcount;
    struct SDL_PixelFormat *next;
} SDL_PixelFormat;

/* Real SDL2 values -- only referenced inside data.h's INIT(...) macro, which expands to
   nothing when BODY isn't defined (i.e. every parse except data.c's own, which no longer
   exists as C at all -- see globals.rs), so these likely aren't even evaluated during a
   real bindgen parse. Defined anyway for correctness/robustness rather than relying on
   that. */
#define SDL_NUM_SCANCODES 512
#define SDL_SCANCODE_LEFT 80
#define SDL_SCANCODE_RIGHT 79
#define SDL_SCANCODE_UP 82
#define SDL_SCANCODE_DOWN 81
#define SDL_SCANCODE_HOME 74
#define SDL_SCANCODE_PAGEUP 75
#define SDL_SCANCODE_RSHIFT 229
#define SDL_SCANCODE_RETURN 40
#define SDL_SCANCODE_ESCAPE 41

#endif /* SDLPOP_SDL_STUB_H */
