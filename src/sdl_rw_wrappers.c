#include "common.h"

int process_rw_write(SDL_RWops* rw, void* data, size_t data_size) {
	return (int)SDL_RWwrite(rw, data, data_size, 1);
}

int process_rw_read(SDL_RWops* rw, void* data, size_t data_size) {
	return (int)SDL_RWread(rw, data, data_size, 1);
}

// never_is_16_list is defined here (not in the Rust port of options.c) because
// menu.c holds an extern reference to it for the in-game settings UI.
KEY_VALUE_LIST(never_is_16, {{"Never", 16}});
