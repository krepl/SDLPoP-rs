//! Cutscene playback and the princess-room scenes — a port of `seg001.c`.
//!
//! Every cutscene in the game is played inside the princess's room, which is
//! drawn from the `PV.DAT` archive rather than from level data. The module has
//! four loosely separable parts:
//!
//! * **The frame pump.** [`proc_cutscene_frame`] is the heart of the module: it
//!   advances both characters' animation sequences, redraws the room, services
//!   the flash/fade effects and the sound queue, then busy-waits on `timer_0`
//!   until the frame's time budget is spent. It returns non-zero when the
//!   cutscene was cut short — `1` for a user interrupt (pause/quit) and `2`
//!   when a global palette fade finished. Callers propagate that by returning
//!   immediately, which is why every cutscene body is a long chain of
//!   `if proc_cutscene_frame(n) != 0 { return; }`.
//!
//! * **Actor setup helpers.** `init_princess`, `init_vizier`, `init_mouse_go`
//!   and friends stamp a character id, position, direction and starting
//!   animation sequence into the shared `Char` scratch slot, which the caller
//!   then commits to `Kid` (`savekid`) or `Guard` (`saveshad`).
//!
//! * **The scenes themselves.** [`cutscene_2_6`], [`cutscene_4`],
//!   [`cutscene_8`], [`cutscene_9`], [`cutscene_12`], [`pv_scene`],
//!   [`time_expired`] and [`end_sequence_anim`] are scripts written against the
//!   two groups above. [`load_intro`] loads the backdrop chtabs, calls one of
//!   these through a function pointer, and tears the chtabs back down.
//!
//! * **The Hall of Fame.** [`end_sequence`] runs the winning cutscene, then
//!   inserts the player's time into the `PRINCE.HOF` leaderboard, prompts for a
//!   name, and writes the file back out.
//!
//! Faithful-port note: `graphics_mode` is always `gmMcgaVga` in SDLPoP, so the
//! CGA/Hercules arms retained below are dead code kept for structural fidelity
//! with the C source.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_short, c_void};
use super::*;
use crate::platform::Renderer;


// File-local state (declared as globals in seg001.c, not in data.c).

/// Frames still to be played by the current [`proc_cutscene_frame`] call.
/// Also doubles as the parity source for the twinkling stars (data:4CB4).
static mut cutscene_wait_frames: c_short = 0;
/// Length of one cutscene frame in timer ticks; scenes retune it mid-script
/// to speed the action up or slow it down (data:3D14).
static mut cutscene_frame_time: c_short = 0;
/// When set, the pause/quit key check in [`proc_cutscene_frame`] is skipped so
/// the scene cannot be interrupted (data:588C).
static mut disable_keys: c_short = 0;
/// Animation phase of the sand falling through the hourglass, cycling 0..=2.
/// A negative value means "no sand flowing" (data:436A).
static mut hourglass_sandflow: c_short = 0;
/// How full the hourglass is drawn, 0 (absent) through 7 (empty) (data:5964).
static mut hourglass_state: c_short = 0;
/// Which of the two princess-room torches gets animated this call; flipped on
/// every iteration of [`princess_room_torch`] (data:4CC4).
static mut which_torch: c_short = 0;

/// One Hall of Fame entry, laid out exactly as stored in `PRINCE.HOF`.
///
/// `#pragma pack(push,1)` in the C source makes this 29 bytes with no padding,
/// and the file format depends on that, so the Rust mirror must stay
/// `repr(C, packed)`. Fields are read/written by value only — taking a
/// reference to `min` or `tick` would be unaligned and therefore UB.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct hof_type {
    name: [c_char; 25],
    min: c_short,
    tick: c_short,
}

const MAX_HOF_COUNT: usize = 6;
const N_STARS: usize = 6;
const N_STAR_COLORS: usize = 5;

static mut hof: [hof_type; MAX_HOF_COUNT] = [
    hof_type { name: [0; 25], min: 0, tick: 0 },
    hof_type { name: [0; 25], min: 0, tick: 0 },
    hof_type { name: [0; 25], min: 0, tick: 0 },
    hof_type { name: [0; 25], min: 0, tick: 0 },
    hof_type { name: [0; 25], min: 0, tick: 0 },
    hof_type { name: [0; 25], min: 0, tick: 0 },
];

/// Screen rectangles of the six Hall of Fame text rows (data:0D92).
// data:0D92
static hof_rects: [rect_type; MAX_HOF_COUNT] = [
    rect_type { top:  84, left:  72, bottom:  96, right: 248 },
    rect_type { top:  98, left:  72, bottom: 110, right: 248 },
    rect_type { top: 112, left:  72, bottom: 124, right: 248 },
    rect_type { top: 126, left:  72, bottom: 138, right: 248 },
    rect_type { top: 140, left:  72, bottom: 152, right: 248 },
    rect_type { top: 154, left:  72, bottom: 166, right: 248 },
];

/// Remaining-minute thresholds that pick the hourglass fill level (data:0DEC).
/// See [`hourglass_frame`].
static time_bound: [c_short; 4] = [6, 17, 33, 65];

// data:0DF4 / 0DF8 / 0DFC — positions and current frames of the two torches.
// The C source declares the position tables as `short[]`, but they are only
// ever passed to `add_backtable`, whose xh/xl parameters are `sbyte`; all four
// values fit, so storing them as `i8` avoids a cast at the call site.
static princess_torch_pos_xh: [i8; 2] = [11, 26];
static princess_torch_pos_xl: [i8; 2] = [5, 3];
static mut princess_torch_frame: [c_short; 2] = [1, 6];

/// One twinkling star in the window of the princess's room.
struct star_type {
    x: c_short,
    y: c_short,
    /// Index into [`star_colors`], advanced every time the star is redrawn.
    color: c_short,
}

// data:0DC2
static mut stars: [star_type; N_STARS] = [
    star_type { x: 20, y:  97, color: 0 },
    star_type { x: 16, y: 104, color: 1 },
    star_type { x: 23, y: 110, color: 2 },
    star_type { x: 17, y: 116, color: 3 },
    star_type { x: 24, y: 120, color: 4 },
    star_type { x: 18, y: 128, color: 0 },
];

// data:0DE6
static star_colors: [u8; N_STAR_COLORS] = [8, 7, 15, 15, 7];

static hof_file: &[u8] = b"PRINCE.HOF\0";

/// Plays `wait_frames` frames of the current cutscene.
///
/// Each frame advances both characters' animation sequences, repaints the
/// princess room, runs one step of the flash and global-fade effects, and then
/// busy-waits until `timer_0` expires so the scene runs at `cutscene_frame_time`
/// ticks per frame.
///
/// Returns `0` if all the requested frames played out, `1` if the player paused
/// or quit (the caller must abandon the scene), and `2` if a global palette fade
/// completed — which is how [`fade_in_1`] and [`fade_out_1`] learn they are done.
// seg001:0004
#[no_mangle]
pub unsafe extern "C" fn proc_cutscene_frame(wait_frames: c_int) -> c_int {
    cutscene_wait_frames = wait_frames as c_short;
    reset_timer(timerids_timer_0 as c_int);
    loop {
        set_timer_length(timerids_timer_0 as c_int, cutscene_frame_time as c_int);
        play_both_seq();
        draw_proom_drects(); // changed order of drects and flash
        if flash_time != 0 {
            do_flash(flash_color as c_short);
        }
        if flash_time != 0 {
            flash_time -= 1;
            remove_flash();
        }
        if check_sound_playing() == 0 {
            play_next_sound();
        }
        // Busy-waiting loop: spin until timer_0 stops, stepping the fade (or
        // idling) once per pass.
        loop {
            if disable_keys == 0 && do_paused() != 0 {
                stop_sounds();
                draw_rect(&screen_rect, colorids_color_0_black as c_int);
                if is_global_fading != 0 {
                    restore_and_free_fade_buffer();
                }
                return 1;
            }
            if is_global_fading != 0 {
                let fade_finished = (*fade_palette_buffer)
                    .proc_fade_frame
                    .map(|f| f(fade_palette_buffer))
                    .unwrap_or(0);
                if fade_finished != 0 {
                    restore_and_free_fade_buffer();
                    return 2;
                }
            } else {
                idle();
                delay_ticks(1);
            }
            if has_timer_stopped(timerids_timer_0 as c_int) != 0 {
                break;
            }
        }
        cutscene_wait_frames -= 1;
        if cutscene_wait_frames == 0 {
            break;
        }
    }
    0
}

/// Tears down the in-progress global palette fade and clears `is_global_fading`.
///
/// Mirrors the `fade_palette_buffer->proc_restore_free(...)` / `is_global_fading = 0`
/// pair that appears twice in [`proc_cutscene_frame`].
unsafe fn restore_and_free_fade_buffer() {
    if let Some(f) = (*fade_palette_buffer).proc_restore_free {
        f(fade_palette_buffer);
    }
    is_global_fading = 0;
}

/// Advances the animation sequences of both on-screen cutscene actors.
// seg001:00DD
#[no_mangle]
pub unsafe extern "C" fn play_both_seq() {
    play_kid_seq();
    play_opp_seq();
}

/// Repaints the princess room and flushes the dirty-rectangle list to screen.
///
/// While a global fade is running the dirty rects are dropped rather than
/// blitted (the fade repaints the whole screen anyway). One star is retwinkled
/// on every other frame.
// seg001:00E6
#[no_mangle]
pub unsafe extern "C" fn draw_proom_drects() {
    draw_princess_room_bg();
    if is_global_fading == 0 {
        while drects_count != 0 {
            drects_count -= 1;
            copy_screen_rect(&drects[drects_count as usize]);
        }
    }
    drects_count = 0;
    if cutscene_wait_frames & 1 != 0 {
        draw_star(prandom(N_STARS as u16 - 1) as c_int, 1);
    }
}

/// Advances the kid's (or the mouse's) animation sequence by one frame.
// seg001:0128
#[no_mangle]
pub unsafe extern "C" fn play_kid_seq() {
    loadkid();
    if Char.frame != 0 {
        play_seq();
        savekid();
    }
}

/// Advances the opponent's (princess or vizier) animation sequence by one frame.
// seg001:013F
#[no_mangle]
pub unsafe extern "C" fn play_opp_seq() {
    loadshad_and_opp();
    if Char.frame != 0 {
        play_seq();
        saveshad();
    }
}

/// Builds one frame of the princess room into the draw tables.
///
/// Queues both actors as objects, then the pillar in front of them, the two
/// torches and the hourglass, and finally hands the assembled tables to
/// `draw_tables`.
// seg001:0156
#[no_mangle]
pub unsafe extern "C" fn draw_princess_room_bg() {
    table_counts.fill(0);
    loadkid();
    if Char.frame != 0 {
        load_frame_to_obj();
        obj_tilepos = 30;
        add_objtable(0);
    }
    loadshad();
    if Char.frame != 0 {
        load_frame_to_obj();
        obj_tilepos = 30;
        add_objtable(0);
    }
    redraw_needed_tiles();
    add_foretable(
        chtabs_id_chtab_8_princessroom as c_short,
        2, // pillar piece
        30,
        0,
        167,
        blitters_blitters_10h_transp as c_int,
        0,
    );
    princess_room_torch();
    draw_hourglass();
    draw_tables();
}

/// Switches the opponent (`Guard`) to animation sequence `seq_index`.
// seg001:01E0
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_shad_char(seq_index: c_int) {
    loadshad();
    seqtbl_offset_char(seq_index as c_short);
    saveshad();
}

/// Switches the kid (`Kid`) to animation sequence `seq_index`.
// seg001:01F9
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_kid_char(seq_index: c_int) {
    loadkid();
    seqtbl_offset_char(seq_index as c_short);
    savekid();
}

/// Places the mouse where cutscene 8 starts it: walking, at x=144.
// seg001:0212
#[no_mangle]
pub unsafe extern "C" fn init_mouse_cu8() {
    init_mouse_go();
    Char.x = 144;
    seqtbl_offset_char(seqids_seq_106_mouse as c_short); // mouse
    play_seq();
}

/// Puts the mouse at the right edge of the room, walking left.
// seg001:022A
#[no_mangle]
pub unsafe extern "C" fn init_mouse_go() {
    Char.charid = charids_charid_24_mouse as u8;
    Char.x = 199;
    Char.y = 167;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_105_mouse_forward as c_short); // mouse go
    play_seq();
}

/// Puts the princess in her crouching pose (PV2 backdrop).
// seg001:024D
#[no_mangle]
pub unsafe extern "C" fn princess_crouching() {
    init_princess();
    Char.x = 131;
    Char.y = 169;
    seqtbl_offset_char(seqids_seq_110_princess_crouching_PV2 as c_short); // princess crouching [PV2]
    play_seq();
}

/// Puts the princess standing and facing right (PV2 backdrop).
// seg001:026A
#[no_mangle]
pub unsafe extern "C" fn princess_stand() {
    init_princess_right();
    Char.x = 144;
    Char.y = 169;
    seqtbl_offset_char(seqids_seq_94_princess_stand_PV1 as c_short); // princess stand [PV1]
    play_seq();
}

/// Standing princess shifted right, as cutscene 12 wants her.
// seg001:0287
#[no_mangle]
pub unsafe extern "C" fn init_princess_x156() {
    init_princess();
    Char.x = 156;
}

/// Puts the princess lying on her bed (PV2 backdrop).
// seg001:0291
#[no_mangle]
pub unsafe extern "C" fn princess_lying() {
    init_princess();
    Char.x = 92;
    Char.y = 162;
    seqtbl_offset_char(seqids_seq_103_princess_lying_PV2 as c_short); // princess lying [PV2]
    play_seq();
}

/// [`init_princess`], but facing right instead of left.
// seg001:02AE
#[no_mangle]
pub unsafe extern "C" fn init_princess_right() {
    init_princess();
    Char.direction = directions_dir_0_right as i8;
}

/// Places the princess for the end sequence, waiting for the kid to arrive.
// seg001:02B8
#[no_mangle]
pub unsafe extern "C" fn init_ending_princess() {
    init_princess();
    Char.x = 136;
    Char.y = 164;
    seqtbl_offset_char(seqids_seq_109_princess_stand_PV2 as c_short); // princess standing [PV2]
    play_seq();
}

/// Places the mouse for its cameo at the end of the ending sequence.
// seg001:02D5
#[no_mangle]
pub unsafe extern "C" fn init_mouse_1() {
    init_mouse_go();
    Char.x = Char.x.wrapping_sub(2);
    Char.y = 164;
}

/// Default princess placement: standing at x=120, facing left.
// seg001:02E4
#[no_mangle]
pub unsafe extern "C" fn init_princess() {
    Char.charid = charids_charid_5_princess as u8;
    Char.x = 120;
    Char.y = 166;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_94_princess_stand_PV1 as c_short); // princess stand [PV1]
    play_seq();
}

/// Places Jaffar (the vizier) at the right edge of the room, facing left.
// seg001:0307
#[no_mangle]
pub unsafe extern "C" fn init_vizier() {
    Char.charid = charids_charid_6_vizier as u8;
    Char.x = 198;
    Char.y = 166;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_95_Jaffar_stand_PV1 as c_short); // Jaffar stand [PV1]
    play_seq();
}

/// Places the kid running in from the right for the end sequence.
// seg001:032A
#[no_mangle]
pub unsafe extern "C" fn init_ending_kid() {
    Char.charid = charids_charid_0_kid as u8;
    Char.x = 198;
    Char.y = 164;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_1_start_run as c_short); // start run
    play_seq();
}

/// Cutscene after level 8: the mouse stands up and leaves, the princess rises.
// seg001:034D
#[no_mangle]
pub unsafe extern "C" fn cutscene_8() {
    play_sound(soundids_sound_35_cutscene_8_9 as c_int); // cutscene 8, 9
    set_hourglass_state(hourglass_frame());
    init_mouse_cu8();
    savekid();
    princess_crouching();
    saveshad();
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(20) != 0 { return; }
    seqtbl_offset_kid_char(seqids_seq_107_mouse_stand_up_and_go as c_int); // mouse stand up and go
    if proc_cutscene_frame(20) != 0 { return; }
    seqtbl_offset_shad_char(seqids_seq_111_princess_stand_up_PV2 as c_int); // princess stand up [PV2]
    if proc_cutscene_frame(20) != 0 { return; }
    Kid.frame = 0;
    fade_out_1();
}

/// Cutscene after level 9: the princess crouches down to the mouse.
// seg001:03B7
#[no_mangle]
pub unsafe extern "C" fn cutscene_9() {
    play_sound(soundids_sound_35_cutscene_8_9 as c_int); // cutscene 8, 9
    set_hourglass_state(hourglass_frame());
    princess_stand();
    saveshad();
    if fade_in_1() != 0 { return; }
    init_mouse_go();
    savekid();
    if proc_cutscene_frame(5) != 0 { return; }
    seqtbl_offset_shad_char(seqids_seq_112_princess_crouch_down_PV2 as c_int); // princess crouch down [PV2]
    if proc_cutscene_frame(9) != 0 { return; }
    seqtbl_offset_kid_char(seqids_seq_114_mouse_stand as c_int); // mouse stand
    if proc_cutscene_frame(58) != 0 { return; }
    fade_out_1();
}

/// The winning animation: the kid runs in, the princess turns and hugs him,
/// and (unless the mod disables it) the mouse makes a final appearance.
///
/// Keys are disabled for the whole scene, and the sound is force-enabled if the
/// player had turned it off.
// seg001:041C
#[no_mangle]
pub unsafe extern "C" fn end_sequence_anim() {
    disable_keys = 1;
    if is_sound_on == 0 {
        turn_sound_on_off(0x0F);
    }
    copy_screen_rect(&screen_rect);
    play_sound(soundids_sound_26_embrace as c_int); // arrived to princess
    init_ending_princess();
    saveshad();
    init_ending_kid();
    savekid();
    if proc_cutscene_frame(8) != 0 { return; }
    seqtbl_offset_shad_char(seqids_seq_108_princess_turn_and_hug as c_int); // princess turn and hug [PV2]
    if proc_cutscene_frame(5) != 0 { return; }
    seqtbl_offset_kid_char(seqids_seq_13_stop_run as c_int); // stop run
    if proc_cutscene_frame(2) != 0 { return; }
    Kid.frame = 0;
    if proc_cutscene_frame(39) != 0 { return; }
    if (*custom).no_mouse_in_ending == 0 {
        init_mouse_1();
        savekid();
        if proc_cutscene_frame(9) != 0 { return; }
        seqtbl_offset_kid_char(seqids_seq_101_mouse_stands_up as c_int); // mouse stands up
        if proc_cutscene_frame(41) != 0 { return; }
    }
    fade_out_1();
    while check_sound_playing() != 0 {
        idle();
        delay_ticks(1);
    }
}

/// The "you ran out of time" scene: the hourglass is shown fully empty with the
/// sand stopped, and the game holds on that image for about 100 frames.
// seg001:04D3
#[no_mangle]
pub unsafe extern "C" fn time_expired() {
    disable_keys = 1;
    set_hourglass_state(7);
    hourglass_sandflow = -1;
    play_sound(soundids_sound_36_out_of_time as c_int); // time over
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(2) != 0 { return; }
    if proc_cutscene_frame(100) != 0 { return; }
    fade_out_1();
    while check_sound_playing() != 0 {
        idle();
        do_paused();
        delay_ticks(1);
    }
}

/// Cutscene after level 12. If the player is running low on time (hourglass
/// frame 6, i.e. under six minutes left) the princess turns around in alarm;
/// otherwise this degrades to the generic [`cutscene_2_6`].
// seg001:0525
#[no_mangle]
pub unsafe extern "C" fn cutscene_12() {
    let frame_num = hourglass_frame() as c_short;
    if frame_num < 6 {
        cutscene_2_6();
        return;
    }
    set_hourglass_state(frame_num as c_int);
    init_princess_x156();
    saveshad();
    play_sound(soundids_sound_40_cutscene_12_short_time as c_int); // cutscene 12 short time
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(2) != 0 { return; }
    seqtbl_offset_shad_char(98); // princess turn around [PV1]
    if proc_cutscene_frame(24) != 0 { return; }
    fade_out_1();
}

/// Cutscene after level 4: the princess is lying on her bed.
// seg001:0584
#[no_mangle]
pub unsafe extern "C" fn cutscene_4() {
    play_sound(soundids_sound_27_cutscene_2_4_6_12 as c_int); // cutscene 2, 4, 6, 12
    set_hourglass_state(hourglass_frame());
    princess_lying();
    saveshad();
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(26) != 0 { return; }
    fade_out_1();
}

/// Generic "princess waiting" cutscene, used after levels 2 and 6 and as the
/// fallback arm of [`cutscene_12`].
// seg001:05B8
#[no_mangle]
pub unsafe extern "C" fn cutscene_2_6() {
    play_sound(soundids_sound_27_cutscene_2_4_6_12 as c_int); // cutscene 2, 4, 6, 12
    set_hourglass_state(hourglass_frame());
    init_princess_right();
    saveshad();
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(26) != 0 { return; }
    fade_out_1();
}

/// Plays cutscene frames one at a time until the currently queued sound has
/// finished, keeping the animation running underneath the narration.
///
/// Returns `true` if the scene was interrupted and the caller must bail out.
unsafe fn play_frames_while_sound_playing() -> bool {
    loop {
        if proc_cutscene_frame(1) != 0 {
            return true;
        }
        if check_sound_playing() == 0 {
            return false;
        }
    }
}

/// The story intro ("PV" = princess/vizier): the princess waits, Jaffar walks
/// in, casts the spell that starts the hourglass running, and leaves.
///
/// This is by far the longest script in the module; the `cutscene_frame_time`
/// assignments partway through are deliberate pacing changes from the original.
// seg001:05EC
#[no_mangle]
pub unsafe extern "C" fn pv_scene() {
    init_princess();
    saveshad();
    if fade_in_1() != 0 { return; }
    init_vizier();
    savekid();
    if proc_cutscene_frame(2) != 0 { return; }
    play_sound(soundids_sound_50_story_2_princess as c_int); // story 2: princess waiting
    if play_frames_while_sound_playing() { return; }
    cutscene_frame_time = 8;
    if proc_cutscene_frame(5) != 0 { return; }
    play_sound(soundids_sound_4_gate_closing as c_int); // gate closing
    if play_frames_while_sound_playing() { return; }
    play_sound(soundids_sound_51_princess_door_opening as c_int); // princess door opening
    if proc_cutscene_frame(3) != 0 { return; }
    seqtbl_offset_shad_char(98); // princess turn around [PV1]
    if proc_cutscene_frame(5) != 0 { return; }
    seqtbl_offset_kid_char(96); // Jaffar walk [PV1]
    if proc_cutscene_frame(6) != 0 { return; }
    play_sound(soundids_sound_53_story_3_Jaffar_comes as c_int); // story 3: Jaffar comes
    seqtbl_offset_kid_char(97); // Jaffar stop [PV1]
    if proc_cutscene_frame(4) != 0 { return; }
    if proc_cutscene_frame(18) != 0 { return; }
    seqtbl_offset_kid_char(96); // Jaffar walk [PV1]
    if proc_cutscene_frame(30) != 0 { return; }
    seqtbl_offset_kid_char(97); // Jaffar stop [PV1]
    if proc_cutscene_frame(35) != 0 { return; }
    seqtbl_offset_kid_char(102); // Jaffar conjuring [PV1]
    cutscene_frame_time = 7;
    if proc_cutscene_frame(1) != 0 { return; }
    seqtbl_offset_shad_char(99); // princess step back [PV1]
    if proc_cutscene_frame(17) != 0 { return; }
    hourglass_state = 1;
    flash_time = 5;
    flash_color = 15; // white
    if play_frames_while_sound_playing() { return; }
    seqtbl_offset_kid_char(100); // Jaffar end conjuring and walk [PV1]
    hourglass_sandflow = 0;
    if proc_cutscene_frame(6) != 0 { return; }
    play_sound(soundids_sound_52_story_4_Jaffar_leaves as c_int); // story 4: Jaffar leaves
    if proc_cutscene_frame(24) != 0 { return; }
    hourglass_state = 2;
    if proc_cutscene_frame(9) != 0 { return; }
    seqtbl_offset_shad_char(113); // princess look down [PV1]
    if proc_cutscene_frame(28) != 0 { return; }
    fade_out_1();
}

/// Shows the hourglass at fill level `state` and restarts the sand animation.
// seg001:07C7
#[no_mangle]
pub unsafe extern "C" fn set_hourglass_state(state: c_int) {
    hourglass_sandflow = 0;
    hourglass_state = state as c_short;
}

/// Picks the hourglass fill level from the minutes still left on the clock.
///
/// The result is `6 - n`, where `n` is how many of the [`time_bound`]
/// thresholds `rem_min` has already reached: 6 (nearly empty, under six minutes
/// left) down to 2 (nearly full, an hour or more left).
// seg001:07DA
#[no_mangle]
pub unsafe extern "C" fn hourglass_frame() -> c_int {
    let bound_index = time_bound
        .iter()
        .position(|&bound| bound > rem_min)
        .unwrap_or(time_bound.len());
    6 - bound_index as c_int
}

/// Queues both princess-room torches into the back draw table, advancing one
/// flame animation per call.
///
/// The C source runs the body twice per call and flips `which_torch` each time,
/// so both torches are drawn but only one has its frame advanced — the other is
/// redrawn with the frame it already had. That alternation is what makes the two
/// flames flicker out of step.
// seg001:0808
#[no_mangle]
pub unsafe extern "C" fn princess_room_torch() {
    for _ in 0..2 {
        which_torch = if which_torch == 0 { 1 } else { 0 };
        let wt = which_torch as usize;
        princess_torch_frame[wt] = get_torch_frame(princess_torch_frame[wt]);
        add_backtable(
            chtabs_id_chtab_1_flameswordpotion as c_short,
            princess_torch_frame[wt] as c_int + 1,
            princess_torch_pos_xh[wt],
            princess_torch_pos_xl[wt],
            116,
            0,
            0,
        );
    }
}

/// Queues the hourglass and its falling sand into the draw tables.
///
/// A negative `hourglass_sandflow` means the sand has stopped (the clock is not
/// running yet, or has run out) and no sand sprite is drawn. At fill level 7 the
/// hourglass is empty, so the sand sprite is suppressed and the C source's early
/// `return` skips the hourglass body as well — but only on the sand-flowing
/// path. [`time_expired`] deliberately pairs state 7 with a stopped sandflow so
/// the empty hourglass still gets drawn.
// seg001:0863
#[no_mangle]
pub unsafe extern "C" fn draw_hourglass() {
    if hourglass_sandflow >= 0 {
        hourglass_sandflow = ((hourglass_sandflow as i32 + 1) % 3) as c_short;
        if hourglass_state >= 7 { return; }
        add_foretable(
            chtabs_id_chtab_8_princessroom as c_short,
            hourglass_sandflow as c_int + 10,
            20,
            0,
            164,
            blitters_blitters_10h_transp as c_int,
            0,
        );
    }
    if hourglass_state != 0 {
        add_midtable(
            chtabs_id_chtab_8_princessroom as c_short,
            hourglass_state as c_int + 2,
            19,
            0,
            168,
            blitters_blitters_10h_transp as c_int,
            1,
        );
    }
}

/// Clears all per-cutscene state so the next scene starts from a clean slate.
// seg001:08CA
#[no_mangle]
pub unsafe extern "C" fn reset_cutscene() {
    Guard.frame = 0;
    Kid.frame = 0;
    which_torch = 0;
    disable_keys = 0;
    hourglass_state = 0;
    // memset(byte_1ED6E, 0, 8); // not used elsewhere
    hourglass_sandflow = -1;
    cutscene_frame_time = 6;
    clear_tile_wipes();
    next_sound = -1;
}

/// Flashes the whole screen in `color` for two timer ticks.
///
/// The redundant inner `color != 0` re-check is preserved from the C source; it
/// is unreachable-as-false because the outer guard already excludes zero.
// seg001:0908
#[no_mangle]
pub unsafe extern "C" fn do_flash(color: c_short) {
    if color != 0 && graphics_mode == grmodes_gmMcgaVga as u8 {
        reset_timer(timerids_timer_2 as c_int);
        set_timer_length(timerids_timer_2 as c_int, 2);
        set_bg_attr(0, color as c_int);
        if color != 0 {
            do_simple_wait(timerids_timer_2 as c_int); // give some time to show the flash
        }
    }
}

/// Sleeps for `ticks` game ticks (1/60 s each), unless a replay is being
/// fast-forwarded.
#[no_mangle]
pub unsafe extern "C" fn delay_ticks(ticks: u32) {
    if replaying != 0 && skipping_replay != 0 { return; }
    // C computes this in Uint32 and lets it wrap; wrapping_mul keeps that exact
    // behaviour instead of panicking in a Rust debug build.
    crate::platform::sdl::shared_renderer().delay(ticks.wrapping_mul(1000 / 60));
}

/// Undoes the screen flash set up by [`do_flash`].
// seg001:0981
#[no_mangle]
pub unsafe extern "C" fn remove_flash() {
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        set_bg_attr(0, 0);
    }
}

/// Runs the whole winning finale: the ending cutscene, the story/title images,
/// and then the Hall of Fame insertion and name prompt.
///
/// The player's time is inserted into `hof` at the first position it beats
/// (later entries shift down), the name is read interactively, `PRINCE.HOF` is
/// rewritten, and finally the game restarts at the title screen.
// seg001:09D7
#[no_mangle]
pub unsafe extern "C" fn end_sequence() {
    let mut rect = rect_type { top: 0, left: 0, bottom: 0, right: 0 };
    let mut color: c_short = 0;
    let mut bgcolor: c_short = 15;
    load_intro(1, Some(end_sequence_anim), 1);
    clear_screen_and_sounds();
    is_ending_sequence = true; // added (fix being able to pause the game during the end sequence)
    load_opt_sounds(
        soundids_sound_56_ending_music as c_int,
        soundids_sound_56_ending_music as c_int,
    ); // winning theme
    play_sound_from_buffer(sound_pointers[soundids_sound_56_ending_music as usize]); // winning theme
    if !offscreen_surface.is_null() { free_surface(offscreen_surface); } // missing in original
    offscreen_surface = make_offscreen_buffer(&screen_rect);
    load_title_images(0);
    current_target_surface = offscreen_surface;
    draw_full_image(full_image_id_STORY_FRAME);
    draw_full_image(full_image_id_STORY_HAIL);
    fade_in_2(offscreen_surface, 0x800);
    pop_wait(timerids_timer_0 as c_int, 900);
    start_timer(timerids_timer_0 as c_int, 240);
    draw_full_image(full_image_id_TITLE_MAIN);
    transition_ltr();
    do_wait(timerids_timer_0 as c_int);

    // Find the first leaderboard slot the player's time beats. `rem_tick` is a
    // C `word`, so the C comparison promotes it to `int` (zero-extended)
    // alongside the signed `tick` field — widen both here rather than narrowing
    // `rem_tick` to `short`, which would sign-extend values above 0x7FFF.
    let mut hof_index: c_short = 0;
    while hof_index < hof_count {
        let entry = hof[hof_index as usize];
        let (entry_min, entry_tick) = ({ entry.min }, { entry.tick });
        if entry_min < rem_min
            || (entry_min == rem_min && (entry_tick as c_int) < rem_tick as c_int)
        {
            break;
        }
        hof_index += 1;
    }

    if hof_index < MAX_HOF_COUNT as c_short && hof_index <= hof_count {
        fade_out_2(0x1000);
        // Shift the entries below the insertion point down by one, then write
        // the new entry into the hole that leaves at `hof_index`.
        let insert_at = hof_index as usize;
        for i in (insert_at + 1..MAX_HOF_COUNT).rev() {
            hof[i] = hof[i - 1];
        }
        hof[insert_at].name[0] = 0;
        hof[insert_at].min = rem_min;
        hof[insert_at].tick = rem_tick as c_short;
        if hof_count < MAX_HOF_COUNT as c_short {
            hof_count += 1;
        }
        draw_full_image(full_image_id_STORY_FRAME);
        draw_full_image(full_image_id_HOF_POP);
        show_hof();
        offset4_rect_add(&mut rect, &hof_rects[insert_at], -4, -1, -40, -1);
        let peel = read_peel_from_screen(&rect);
        if graphics_mode == grmodes_gmMcgaVga as u8 {
            color = 0xBE;
            bgcolor = 0xB7;
        }
        draw_rect(&rect, bgcolor as c_int);
        fade_in_2(offscreen_surface, 0x1800);
        current_target_surface = onscreen_surface_;
        let name_ptr = hof[insert_at].name.as_mut_ptr();
        while input_str(
            &rect,
            name_ptr,
            24,
            b"\0".as_ptr() as *const c_char,
            0,
            4,
            color as c_int,
            bgcolor as c_int,
        ) <= 0
        {}
        restore_peel(peel);
        show_hof_text(
            &hof_rects[insert_at] as *const _ as *mut rect_type,
            -1,
            0,
            hof[insert_at].name.as_ptr(),
        );
        hof_write();
        pop_wait(timerids_timer_0 as c_int, 120);
        current_target_surface = offscreen_surface;
        draw_full_image(full_image_id_TITLE_MAIN);
        transition_ltr();
    }
    while check_sound_playing() != 0 && key_test_quit() == 0 {
        idle();
        delay_ticks(1);
    }
    fade_out_2(0x1000);
    start_level = -1;
    is_ending_sequence = false;
    start_game();
}

/// Ends the game because the clock ran out: shows the [`time_expired`] scene
/// (skipped in demo mode) and restarts at the title screen.
// seg001:0C94
#[no_mangle]
pub unsafe extern "C" fn expired() {
    if demo_mode == 0 {
        if !offscreen_surface.is_null() { free_surface(offscreen_surface); } // missing in original
        offscreen_surface = core::ptr::null_mut();
        clear_screen_and_sounds();
        offscreen_surface = make_offscreen_buffer(&screen_rect);
        load_intro(1, Some(time_expired), 1);
    }
    start_level = -1;
    start_game();
}

/// Sets up the princess-room backdrop, plays one cutscene through `func`, then
/// frees the graphics it loaded.
///
/// `which_imgs` selects which 50-image block of `PV.DAT` is loaded into
/// `id_chtab_4` (the story-vs-cutscene variant of Jaffar and the princess), and
/// `free_sounds` asks for the optional sound bank to be released first.
// seg001:0CCD
#[no_mangle]
pub unsafe extern "C" fn load_intro(
    which_imgs: c_int,
    func: Option<unsafe extern "C" fn()>,
    free_sounds: c_int,
) {
    draw_rect(&screen_rect, colorids_color_0_black as c_int);
    if free_sounds != 0 {
        free_optional_sounds();
    }
    free_all_chtabs_from(chtabs_id_chtab_3_princessinstory as c_int);
    load_chtab_from_file(chtabs_id_chtab_8_princessroom as c_int, 950, b"PV.DAT\0".as_ptr() as *const c_char, 1 << 13);
    load_chtab_from_file(chtabs_id_chtab_9_princessbed as c_int, 980, b"PV.DAT\0".as_ptr() as *const c_char, 1 << 14);
    current_target_surface = offscreen_surface;
    method_6_blit_img_to_scr(
        get_image(chtabs_id_chtab_8_princessroom as c_short, 0),
        0,
        0,
        0,
    );
    method_6_blit_img_to_scr(
        get_image(chtabs_id_chtab_9_princessbed as c_short, 0),
        0,
        142,
        blitters_blitters_2_or as c_int,
    );
    // Free the images that are not needed anymore.
    free_all_chtabs_from(chtabs_id_chtab_9_princessbed as c_int);
    let img0 = get_image(chtabs_id_chtab_8_princessroom as c_short, 0);
    crate::platform::sdl::shared_renderer().free_surface(img0);
    if !chtab_addrs[chtabs_id_chtab_8_princessroom as usize].is_null() {
        core::ptr::addr_of_mut!((*chtab_addrs[chtabs_id_chtab_8_princessroom as usize]).images)
            .cast::<*mut SDL_Surface>()
            .write(core::ptr::null_mut());
    }
    load_chtab_from_file(
        chtabs_id_chtab_3_princessinstory as c_int,
        800,
        b"PV.DAT\0".as_ptr() as *const c_char,
        1 << 9,
    );
    load_chtab_from_file(
        chtabs_id_chtab_4_jaffarinstory_princessincutscenes as c_int,
        50 * which_imgs + 850,
        b"PV.DAT\0".as_ptr() as *const c_char,
        1 << 10,
    );
    for current_star in 0..N_STARS as c_int {
        draw_star(current_star, 0);
    }
    current_target_surface = onscreen_surface_;
    while check_sound_playing() != 0 {
        idle();
        do_paused();
        delay_ticks(1);
    }
    need_drects = 1;
    reset_cutscene();
    is_cutscene = 1;
    if let Some(f) = func { f(); }
    is_cutscene = 0;
    free_all_chtabs_from(3);
    draw_rect(&screen_rect, colorids_color_0_black as c_int);
}

/// Draws one of the stars in the window of the princess's room.
///
/// Each redraw advances that star's colour through [`star_colors`], which is
/// what makes them twinkle; `mark_dirty` also queues the pixel for the next
/// dirty-rect flush in [`draw_proom_drects`].
// seg001:0E1C
#[no_mangle]
pub unsafe extern "C" fn draw_star(which_star: c_int, mark_dirty: c_int) {
    // The stars in the window of the princess's room.
    let star = &mut stars[which_star as usize];
    let mut rect = rect_type {
        top: star.y,
        left: star.x,
        bottom: star.y + 1,
        right: star.x + 1,
    };
    let star_color = if graphics_mode != grmodes_gmCga as u8
        && graphics_mode != grmodes_gmHgaHerc as u8
    {
        star.color = (star.color + 1) % N_STAR_COLORS as c_short;
        star_colors[star.color as usize] as c_int
    } else {
        15
    };
    draw_rect(&rect, star_color);
    if mark_dirty != 0 {
        add_drect(&mut rect);
    }
}

/// Draws the six Hall of Fame rows: each player's name on the left and their
/// finishing time on the right.
// seg001:0E94
#[no_mangle]
pub unsafe extern "C" fn show_hof() {
    // Hall of Fame
    for index in 0..hof_count as usize {
        // `min` and `tick` are C `short`s that the format arithmetic below
        // promotes to `int`; widen to i32 up front so `719 - tick` and
        // `-min - 1` cannot overflow a Rust i16 the way C never would.
        let entry = hof[index];
        let hof_min = { entry.min } as c_int;
        let hof_tick = { entry.tick } as c_int;
        println!("index = {index}, hof[index].min = {hof_min}, hof[index].tick = {hof_tick}");

        // ALLOW_INFINITE_TIME (defined in config.h).
        let (minutes, seconds) = if hof_min > 0 {
            // if there was a time limit
            (hof_min - 1, hof_tick / 12)
        } else if hof_min == 0 {
            // if there was a time limit and it expired
            (0, 0)
        } else {
            // negative minutes means time ran 'forward' from 0:00 upwards
            (hof_min.abs() - 1, (719 - hof_tick) / 12)
        };
        let time_text = format!("{minutes}:{seconds:02}");
        let time_text_c = std::ffi::CString::new(time_text).unwrap_or_default();

        let rect = &hof_rects[index] as *const _ as *mut rect_type;
        show_hof_text(rect, -1, 0, hof[index].name.as_ptr());
        show_hof_text(rect, 1, 0, time_text_c.as_ptr());
    }
}

/// Resolves the writable path of the `PRINCE.HOF` leaderboard file.
#[no_mangle]
pub unsafe extern "C" fn get_hof_path(
    custom_path_buffer: *mut c_char,
    max_len: usize,
) -> *const c_char {
    get_writable_file_path(custom_path_buffer, max_len, hof_file.as_ptr() as *const c_char)
}

/// Writes the leaderboard back to `PRINCE.HOF`: a two-byte count followed by
/// the raw six-entry array.
// seg001:0F17
#[no_mangle]
pub unsafe extern "C" fn hof_write() {
    let mut custom_hof_path = [0u8; POP_MAX_PATH as usize];
    let hof_path = get_hof_path(custom_hof_path.as_mut_ptr() as *mut c_char, POP_MAX_PATH as usize);
    let handle = fopen(hof_path, b"wb\0".as_ptr() as *const c_char);
    let ok = !handle.is_null()
        && fwrite(
            &hof_count as *const _ as *const c_void,
            1,
            2,
            handle,
        ) == 2
        && fwrite(
            hof.as_ptr() as *const c_void,
            1,
            core::mem::size_of_val(&hof),
            handle,
        ) == core::mem::size_of_val(&hof);
    if !ok {
        perror(hof_path);
    }
    if !handle.is_null() {
        fclose(handle);
    }
}

/// Loads `PRINCE.HOF`. A missing file leaves the leaderboard empty; a
/// short/corrupt one is reported and also treated as empty.
// seg001:0F6C
#[no_mangle]
pub unsafe extern "C" fn hof_read() {
    hof_count = 0;
    let mut custom_hof_path = [0u8; POP_MAX_PATH as usize];
    let hof_path = get_hof_path(custom_hof_path.as_mut_ptr() as *mut c_char, POP_MAX_PATH as usize);
    let handle = fopen(hof_path, b"rb\0".as_ptr() as *const c_char);
    if handle.is_null() { return; }
    let ok = fread(
        &mut hof_count as *mut _ as *mut c_void,
        1,
        2,
        handle,
    ) == 2
        && fread(
            hof.as_mut_ptr() as *mut c_void,
            1,
            core::mem::size_of_val(&hof),
            handle,
        ) == core::mem::size_of_val(&hof);
    if !ok {
        perror(hof_path);
        hof_count = 0;
    }
    fclose(handle);
}

/// Draws one line of Hall of Fame text with a drop shadow, aligned inside
/// `rect` by `x_align` / `y_align`.
// seg001:0FC3
#[no_mangle]
pub unsafe extern "C" fn show_hof_text(
    rect: *mut rect_type,
    x_align: c_int,
    y_align: c_int,
    text: *const c_char,
) {
    let mut rect2 = rect_type { top: 0, left: 0, bottom: 0, right: 0 };
    let shadow_color: c_int = 0;
    let text_color: c_int = if graphics_mode == grmodes_gmMcgaVga as u8 { 0xB7 } else { 15 };
    // Draw the text twice: once offset by (1,1) in the shadow colour, then on
    // top in the text colour.
    offset2_rect(&mut rect2, rect, 1, 1);
    show_text_with_color(&rect2, x_align, y_align, text, shadow_color);
    show_text_with_color(rect as *const rect_type, x_align, y_align, text, text_color);
}

/// Fades the cutscene in from black, pumping one cutscene frame per fade step.
///
/// Returns `1` if the player interrupted the fade, `0` once it completed.
// seg001:1029
#[no_mangle]
pub unsafe extern "C" fn fade_in_1() -> c_int {
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        fade_palette_buffer = make_pal_buffer_fadein(offscreen_surface, 0x6689, /*0*/ 2);
        is_global_fading = 1;
        if run_global_fade() {
            return 1;
        }
        is_global_fading = 0;
    } else {
        // Faithful-port note: with USE_FADE on, the C source's non-MCGA arm is
        // an empty `// ...` stub, and this blit is the `#else` (no-USE_FADE)
        // body. It is kept here because graphics_mode is always gmMcgaVga in
        // SDLPoP, so this arm is unreachable either way; removing it would be a
        // speculative change to dead code.
        method_1_blit_rect(onscreen_surface_, offscreen_surface, &screen_rect, &screen_rect, 0);
        update_screen();
    }
    0
}

/// Fades the cutscene out to black, pumping one cutscene frame per fade step.
///
/// Returns `1` if the player interrupted the fade, `0` once it completed.
// seg001:112D
#[no_mangle]
pub unsafe extern "C" fn fade_out_1() -> c_int {
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        fade_palette_buffer = make_pal_buffer_fadeout(0x6689, /*0*/ 2);
        is_global_fading = 1;
        if run_global_fade() {
            return 1;
        }
        is_global_fading = 0;
    }
    0
}

/// Drives an already-started global palette fade to completion by pumping
/// single cutscene frames.
///
/// [`proc_cutscene_frame`] returns `2` when the fade's last step ran, and `1`
/// when the player interrupted it; this returns `true` for the latter, meaning
/// the caller must abandon the scene.
unsafe fn run_global_fade() -> bool {
    loop {
        match proc_cutscene_frame(1) {
            0 => continue,
            1 => return true,
            _ => return false,
        }
    }
}

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // hourglass_frame maps remaining minutes to a frame number 2..=6.
    // With rem_min >= time_bound[3]=65 → frame 2; rem_min < time_bound[0]=6 → frame 6.
    #[test]
    fn hourglass_frame_returns_correct_frame() {
        setup();
        unsafe {
            rem_min = 0;   // < 6 → break at index 0 → 6 - 0 = 6
            assert_eq!(hourglass_frame(), 6);
            rem_min = 6;   // >= 6, < 17 → break at index 1 → 6 - 1 = 5
            assert_eq!(hourglass_frame(), 5);
            rem_min = 17;  // >= 17, < 33 → break at index 2 → 6 - 2 = 4
            assert_eq!(hourglass_frame(), 4);
            rem_min = 33;  // >= 33, < 65 → break at index 3 → 6 - 3 = 3
            assert_eq!(hourglass_frame(), 3);
            rem_min = 65;  // >= 65 → loop ends, bound_index=4 → 6 - 4 = 2
            assert_eq!(hourglass_frame(), 2);
        }
    }

    // set_hourglass_state sets hourglass_state and resets sandflow to 0.
    #[test]
    fn set_hourglass_state_updates_state_and_clears_sandflow() {
        setup();
        unsafe {
            hourglass_sandflow = 42;
            set_hourglass_state(5);
            assert_eq!(hourglass_state, 5);
            assert_eq!(hourglass_sandflow, 0);
        }
    }

    // reset_cutscene restores initial state for a new cutscene.
    #[test]
    fn reset_cutscene_restores_defaults() {
        setup();
        unsafe {
            disable_keys = 1;
            hourglass_state = 3;
            hourglass_sandflow = 5;
            which_torch = 1;
            cutscene_frame_time = 99;
            reset_cutscene();
            assert_eq!(disable_keys, 0);
            assert_eq!(hourglass_state, 0);
            assert_eq!(hourglass_sandflow, -1);
            assert_eq!(which_torch, 0);
            assert_eq!(cutscene_frame_time, 6);
        }
    }

    // Not reachable via the replay/trace harness: recording auto-stops before the level-14
    // ending cutscene, and hof_write() is only called from end_sequence() (seg001.c:627),
    // which lives entirely outside play_level_2()'s traced loop. Both hof_write()/hof_read()
    // are pure fopen/fwrite/fread/fclose with no SDL calls, so this drives the real public
    // functions end-to-end.
    #[test]
    fn hof_write_read_roundtrip_preserves_entries() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let scratch = crate::test_support::ScratchDir::new("hof");
        setup();
        unsafe {
            crate::test_support::set_save_path_env(&scratch.0);

            hof_count = 2;
            let mut name0 = [0 as c_char; 25];
            for (i, b) in b"ALICE\0".iter().enumerate() {
                name0[i] = *b as c_char;
            }
            hof[0] = hof_type { name: name0, min: 12, tick: 345 };
            let mut name1 = [0 as c_char; 25];
            for (i, b) in b"BOB\0".iter().enumerate() {
                name1[i] = *b as c_char;
            }
            hof[1] = hof_type { name: name1, min: 7, tick: 89 };

            hof_write();

            // Corrupt in-memory state so the read-back actually proves something.
            hof_count = 0;
            hof[0] = hof_type { name: [0; 25], min: 0, tick: 0 };
            hof[1] = hof_type { name: [0; 25], min: 0, tick: 0 };

            hof_read();

            assert_eq!(hof_count, 2);
            assert_eq!({ hof[0].min }, 12);
            assert_eq!({ hof[0].tick }, 345);
            assert_eq!({ hof[1].min }, 7);
            assert_eq!({ hof[1].tick }, 89);
            assert_eq!(&hof[0].name[..6], &name0[..6]);
            assert_eq!(&hof[1].name[..4], &name1[..4]);

            crate::test_support::remove_save_path_env();
        }
    }
}
