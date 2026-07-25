/*
Not part of the pinned C oracle build (src/CMakeLists.txt / src/Makefile stay untouched).
Generates a real QUICKSAVE.SAV using the actual C quick_save() implementation, for use as
a cross-compatibility fixture in the Rust port's tests (does the Rust port correctly read
a save file the original C code wrote?). Run via scripts/gen_quicksave_fixture.sh.

quick_save() has no SDL calls, so this doesn't need SDL_Init or any game startup sequence.
*/
#include "common.h"

int quick_save(void); // not declared in proto.h

int main(void) {
	current_level = 7;
	hitp_curr = 2;
	Kid.x = 111;

	if (!quick_save()) {
		fprintf(stderr, "quick_save() failed\n");
		return 1;
	}

	printf("wrote QUICKSAVE.SAV: current_level=%d hitp_curr=%d Kid.x=%d\n",
		current_level, hitp_curr, Kid.x);
	return 0;
}
