// Cutscene playback and animation — ported from seg001.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_short, c_void};
use super::*;

extern "C" {
    fn SDL_Delay(ms: u32);
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
}


// File-local state (declared as globals in seg001.c, not in data.c).
static mut cutscene_wait_frames: c_short = 0;
static mut cutscene_frame_time: c_short = 0;
static mut disable_keys: c_short = 0;
static mut hourglass_sandflow: c_short = 0;
static mut hourglass_state: c_short = 0;
static mut which_torch: c_short = 0;

// Hall of Fame entry (pragma pack(1) — no padding).
#[repr(C, packed)]
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

// data:0D92
static hof_rects: [rect_type; MAX_HOF_COUNT] = [
    rect_type { top:  84, left:  72, bottom:  96, right: 248 },
    rect_type { top:  98, left:  72, bottom: 110, right: 248 },
    rect_type { top: 112, left:  72, bottom: 124, right: 248 },
    rect_type { top: 126, left:  72, bottom: 138, right: 248 },
    rect_type { top: 140, left:  72, bottom: 152, right: 248 },
    rect_type { top: 154, left:  72, bottom: 166, right: 248 },
];

// data:0DEC
static time_bound: [c_short; 4] = [6, 17, 33, 65];

// data:0DF4 / 0DF8 / 0DFC
static princess_torch_pos_xh: [i8; 2] = [11, 26];
static princess_torch_pos_xl: [i8; 2] = [5, 3];
static mut princess_torch_frame: [c_short; 2] = [1, 6];

struct star_type {
    x: c_short,
    y: c_short,
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

// seg001:0004
#[no_mangle]
pub unsafe extern "C" fn proc_cutscene_frame(wait_frames: c_int) -> c_int {
    cutscene_wait_frames = wait_frames as c_short;
    reset_timer(timerids_timer_0 as c_int);
    loop {
        set_timer_length(timerids_timer_0 as c_int, cutscene_frame_time as c_int);
        play_both_seq();
        draw_proom_drects();
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
        loop {
            if disable_keys == 0 && do_paused() != 0 {
                stop_sounds();
                draw_rect(&screen_rect, colorids_color_0_black as c_int);
                if is_global_fading != 0 {
                    if let Some(f) = (*fade_palette_buffer).proc_restore_free {
                        f(fade_palette_buffer);
                    }
                    is_global_fading = 0;
                }
                return 1;
            }
            if is_global_fading != 0 {
                let done = (*fade_palette_buffer).proc_fade_frame
                    .map(|f| f(fade_palette_buffer))
                    .unwrap_or(0);
                if done != 0 {
                    if let Some(f) = (*fade_palette_buffer).proc_restore_free {
                        f(fade_palette_buffer);
                    }
                    is_global_fading = 0;
                    return 2;
                }
            } else {
                idle();
                delay_ticks(1);
            }
            if has_timer_stopped(timerids_timer_0 as c_int) != 0 { break; }
        }
        cutscene_wait_frames -= 1;
        if cutscene_wait_frames == 0 { break; }
    }
    0
}

// seg001:00DD
#[no_mangle]
pub unsafe extern "C" fn play_both_seq() {
    play_kid_seq();
    play_opp_seq();
}

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

// seg001:0128
#[no_mangle]
pub unsafe extern "C" fn play_kid_seq() {
    loadkid();
    if Char.frame != 0 {
        play_seq();
        savekid();
    }
}

// seg001:013F
#[no_mangle]
pub unsafe extern "C" fn play_opp_seq() {
    loadshad_and_opp();
    if Char.frame != 0 {
        play_seq();
        saveshad();
    }
}

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

// seg001:01E0
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_shad_char(seq_index: c_int) {
    loadshad();
    seqtbl_offset_char(seq_index as c_short);
    saveshad();
}

// seg001:01F9
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_kid_char(seq_index: c_int) {
    loadkid();
    seqtbl_offset_char(seq_index as c_short);
    savekid();
}

// seg001:0212
#[no_mangle]
pub unsafe extern "C" fn init_mouse_cu8() {
    init_mouse_go();
    Char.x = 144;
    seqtbl_offset_char(seqids_seq_106_mouse as c_short);
    play_seq();
}

// seg001:022A
#[no_mangle]
pub unsafe extern "C" fn init_mouse_go() {
    Char.charid = charids_charid_24_mouse as u8;
    Char.x = 199;
    Char.y = 167;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_105_mouse_forward as c_short);
    play_seq();
}

// seg001:024D
#[no_mangle]
pub unsafe extern "C" fn princess_crouching() {
    init_princess();
    Char.x = 131;
    Char.y = 169;
    seqtbl_offset_char(seqids_seq_110_princess_crouching_PV2 as c_short);
    play_seq();
}

// seg001:026A
#[no_mangle]
pub unsafe extern "C" fn princess_stand() {
    init_princess_right();
    Char.x = 144;
    Char.y = 169;
    seqtbl_offset_char(seqids_seq_94_princess_stand_PV1 as c_short);
    play_seq();
}

// seg001:0287
#[no_mangle]
pub unsafe extern "C" fn init_princess_x156() {
    init_princess();
    Char.x = 156;
}

// seg001:0291
#[no_mangle]
pub unsafe extern "C" fn princess_lying() {
    init_princess();
    Char.x = 92;
    Char.y = 162;
    seqtbl_offset_char(seqids_seq_103_princess_lying_PV2 as c_short);
    play_seq();
}

// seg001:02AE
#[no_mangle]
pub unsafe extern "C" fn init_princess_right() {
    init_princess();
    Char.direction = directions_dir_0_right as i8;
}

// seg001:02B8
#[no_mangle]
pub unsafe extern "C" fn init_ending_princess() {
    init_princess();
    Char.x = 136;
    Char.y = 164;
    seqtbl_offset_char(seqids_seq_109_princess_stand_PV2 as c_short);
    play_seq();
}

// seg001:02D5
#[no_mangle]
pub unsafe extern "C" fn init_mouse_1() {
    init_mouse_go();
    Char.x = Char.x.wrapping_sub(2);
    Char.y = 164;
}

// seg001:02E4
#[no_mangle]
pub unsafe extern "C" fn init_princess() {
    Char.charid = charids_charid_5_princess as u8;
    Char.x = 120;
    Char.y = 166;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_94_princess_stand_PV1 as c_short);
    play_seq();
}

// seg001:0307
#[no_mangle]
pub unsafe extern "C" fn init_vizier() {
    Char.charid = charids_charid_6_vizier as u8;
    Char.x = 198;
    Char.y = 166;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_95_Jaffar_stand_PV1 as c_short);
    play_seq();
}

// seg001:032A
#[no_mangle]
pub unsafe extern "C" fn init_ending_kid() {
    Char.charid = charids_charid_0_kid as u8;
    Char.x = 198;
    Char.y = 164;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_1_start_run as c_short);
    play_seq();
}

// seg001:034D
#[no_mangle]
pub unsafe extern "C" fn cutscene_8() {
    play_sound(soundids_sound_35_cutscene_8_9 as c_int);
    set_hourglass_state(hourglass_frame());
    init_mouse_cu8();
    savekid();
    princess_crouching();
    saveshad();
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(20) != 0 { return; }
    seqtbl_offset_kid_char(seqids_seq_107_mouse_stand_up_and_go as c_int);
    if proc_cutscene_frame(20) != 0 { return; }
    seqtbl_offset_shad_char(seqids_seq_111_princess_stand_up_PV2 as c_int);
    if proc_cutscene_frame(20) != 0 { return; }
    Kid.frame = 0;
    fade_out_1();
}

// seg001:03B7
#[no_mangle]
pub unsafe extern "C" fn cutscene_9() {
    play_sound(soundids_sound_35_cutscene_8_9 as c_int);
    set_hourglass_state(hourglass_frame());
    princess_stand();
    saveshad();
    if fade_in_1() != 0 { return; }
    init_mouse_go();
    savekid();
    if proc_cutscene_frame(5) != 0 { return; }
    seqtbl_offset_shad_char(seqids_seq_112_princess_crouch_down_PV2 as c_int);
    if proc_cutscene_frame(9) != 0 { return; }
    seqtbl_offset_kid_char(seqids_seq_114_mouse_stand as c_int);
    if proc_cutscene_frame(58) != 0 { return; }
    fade_out_1();
}

// seg001:041C
#[no_mangle]
pub unsafe extern "C" fn end_sequence_anim() {
    disable_keys = 1;
    if is_sound_on == 0 {
        turn_sound_on_off(0x0F);
    }
    copy_screen_rect(&screen_rect);
    play_sound(soundids_sound_26_embrace as c_int);
    init_ending_princess();
    saveshad();
    init_ending_kid();
    savekid();
    if proc_cutscene_frame(8) != 0 { return; }
    seqtbl_offset_shad_char(seqids_seq_108_princess_turn_and_hug as c_int);
    if proc_cutscene_frame(5) != 0 { return; }
    seqtbl_offset_kid_char(seqids_seq_13_stop_run as c_int);
    if proc_cutscene_frame(2) != 0 { return; }
    Kid.frame = 0;
    if proc_cutscene_frame(39) != 0 { return; }
    if (*custom).no_mouse_in_ending == 0 {
        init_mouse_1();
        savekid();
        if proc_cutscene_frame(9) != 0 { return; }
        seqtbl_offset_kid_char(seqids_seq_101_mouse_stands_up as c_int);
        if proc_cutscene_frame(41) != 0 { return; }
    }
    fade_out_1();
    while check_sound_playing() != 0 {
        idle();
        delay_ticks(1);
    }
}

// seg001:04D3
#[no_mangle]
pub unsafe extern "C" fn time_expired() {
    disable_keys = 1;
    set_hourglass_state(7);
    hourglass_sandflow = -1;
    play_sound(soundids_sound_36_out_of_time as c_int);
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

// seg001:0525
#[no_mangle]
pub unsafe extern "C" fn cutscene_12() {
    let frame_num = hourglass_frame() as c_short;
    if frame_num >= 6 {
        set_hourglass_state(frame_num as c_int);
        init_princess_x156();
        saveshad();
        play_sound(soundids_sound_40_cutscene_12_short_time as c_int);
        if fade_in_1() != 0 { return; }
        if proc_cutscene_frame(2) != 0 { return; }
        seqtbl_offset_shad_char(98); // princess turn around [PV1]
        if proc_cutscene_frame(24) != 0 { return; }
        fade_out_1();
    } else {
        cutscene_2_6();
    }
}

// seg001:0584
#[no_mangle]
pub unsafe extern "C" fn cutscene_4() {
    play_sound(soundids_sound_27_cutscene_2_4_6_12 as c_int);
    set_hourglass_state(hourglass_frame());
    princess_lying();
    saveshad();
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(26) != 0 { return; }
    fade_out_1();
}

// seg001:05B8
#[no_mangle]
pub unsafe extern "C" fn cutscene_2_6() {
    play_sound(soundids_sound_27_cutscene_2_4_6_12 as c_int);
    set_hourglass_state(hourglass_frame());
    init_princess_right();
    saveshad();
    if fade_in_1() != 0 { return; }
    if proc_cutscene_frame(26) != 0 { return; }
    fade_out_1();
}

// seg001:05EC
#[no_mangle]
pub unsafe extern "C" fn pv_scene() {
    init_princess();
    saveshad();
    if fade_in_1() != 0 { return; }
    init_vizier();
    savekid();
    if proc_cutscene_frame(2) != 0 { return; }
    play_sound(soundids_sound_50_story_2_princess as c_int);
    loop {
        if proc_cutscene_frame(1) != 0 { return; }
        if check_sound_playing() == 0 { break; }
    }
    cutscene_frame_time = 8;
    if proc_cutscene_frame(5) != 0 { return; }
    play_sound(soundids_sound_4_gate_closing as c_int);
    loop {
        if proc_cutscene_frame(1) != 0 { return; }
        if check_sound_playing() == 0 { break; }
    }
    play_sound(soundids_sound_51_princess_door_opening as c_int);
    if proc_cutscene_frame(3) != 0 { return; }
    seqtbl_offset_shad_char(98); // princess turn around [PV1]
    if proc_cutscene_frame(5) != 0 { return; }
    seqtbl_offset_kid_char(96); // Jaffar walk [PV1]
    if proc_cutscene_frame(6) != 0 { return; }
    play_sound(soundids_sound_53_story_3_Jaffar_comes as c_int);
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
    loop {
        if proc_cutscene_frame(1) != 0 { return; }
        if check_sound_playing() == 0 { break; }
    }
    seqtbl_offset_kid_char(100); // Jaffar end conjuring and walk [PV1]
    hourglass_sandflow = 0;
    if proc_cutscene_frame(6) != 0 { return; }
    play_sound(soundids_sound_52_story_4_Jaffar_leaves as c_int);
    if proc_cutscene_frame(24) != 0 { return; }
    hourglass_state = 2;
    if proc_cutscene_frame(9) != 0 { return; }
    seqtbl_offset_shad_char(113); // princess look down [PV1]
    if proc_cutscene_frame(28) != 0 { return; }
    fade_out_1();
}

// seg001:07C7
#[no_mangle]
pub unsafe extern "C" fn set_hourglass_state(state: c_int) {
    hourglass_sandflow = 0;
    hourglass_state = state as c_short;
}

// seg001:07DA
#[no_mangle]
pub unsafe extern "C" fn hourglass_frame() -> c_int {
    let mut bound_index: c_short = 0;
    while bound_index < 4 {
        if time_bound[bound_index as usize] > rem_min {
            break;
        }
        bound_index += 1;
    }
    (6 - bound_index) as c_int
}

// seg001:0808
#[no_mangle]
pub unsafe extern "C" fn princess_room_torch() {
    let mut which: c_short = 2;
    while which > 0 {
        which -= 1;
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

// seg001:08CA
#[no_mangle]
pub unsafe extern "C" fn reset_cutscene() {
    Guard.frame = 0;
    Kid.frame = 0;
    which_torch = 0;
    disable_keys = 0;
    hourglass_state = 0;
    hourglass_sandflow = -1;
    cutscene_frame_time = 6;
    clear_tile_wipes();
    next_sound = -1;
}

// seg001:0908
#[no_mangle]
pub unsafe extern "C" fn do_flash(color: c_short) {
    if color != 0 {
        if graphics_mode == grmodes_gmMcgaVga as u8 {
            reset_timer(timerids_timer_2 as c_int);
            set_timer_length(timerids_timer_2 as c_int, 2);
            set_bg_attr(0, color as c_int);
            if color != 0 { do_simple_wait(timerids_timer_2 as c_int); }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn delay_ticks(ticks: u32) {
    if replaying != 0 && skipping_replay != 0 { return; }
    SDL_Delay(ticks * (1000 / 60));
}

// seg001:0981
#[no_mangle]
pub unsafe extern "C" fn remove_flash() {
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        set_bg_attr(0, 0);
    }
}

// seg001:09D7
#[no_mangle]
pub unsafe extern "C" fn end_sequence() {
    let mut rect = rect_type { top: 0, left: 0, bottom: 0, right: 0 };
    let mut hof_index: c_short = 0;
    let mut color: c_short = 0;
    let bgcolor: c_short = 15;
    load_intro(1, Some(end_sequence_anim), 1);
    clear_screen_and_sounds();
    is_ending_sequence = true;
    load_opt_sounds(
        soundids_sound_56_ending_music as c_int,
        soundids_sound_56_ending_music as c_int,
    );
    play_sound_from_buffer(sound_pointers[soundids_sound_56_ending_music as usize]);
    if !offscreen_surface.is_null() { free_surface(offscreen_surface); }
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
    while hof_index < hof_count {
        let hof_min = core::ptr::addr_of!(hof[hof_index as usize].min).read_unaligned();
        let hof_tick = core::ptr::addr_of!(hof[hof_index as usize].tick).read_unaligned();
        if hof_min < rem_min || (hof_min == rem_min && hof_tick < rem_tick as c_short) {
            break;
        }
        hof_index += 1;
    }
    if hof_index < MAX_HOF_COUNT as c_short && hof_index <= hof_count {
        fade_out_2(0x1000);
        let mut i = 5i16;
        while hof_index + 1 <= i {
            hof[i as usize] = hof_type {
                name: core::ptr::addr_of!(hof[(i - 1) as usize].name).read(),
                min: core::ptr::addr_of!(hof[(i - 1) as usize].min).read_unaligned(),
                tick: core::ptr::addr_of!(hof[(i - 1) as usize].tick).read_unaligned(),
            };
            i -= 1;
        }
        core::ptr::addr_of_mut!(hof[i as usize].name[0]).write(0);
        core::ptr::addr_of_mut!(hof[i as usize].min).write_unaligned(rem_min);
        core::ptr::addr_of_mut!(hof[i as usize].tick).write_unaligned(rem_tick as c_short);
        if hof_count < MAX_HOF_COUNT as c_short {
            hof_count += 1;
        }
        draw_full_image(full_image_id_STORY_FRAME);
        draw_full_image(full_image_id_HOF_POP);
        show_hof();
        offset4_rect_add(&mut rect, &hof_rects[hof_index as usize], -4, -1, -40, -1);
        let peel = read_peel_from_screen(&rect);
        if graphics_mode == grmodes_gmMcgaVga as u8 {
            color = 0xBE;
            // bgcolor = 0xB7 — would shadow outer binding, use local scope
        }
        let bgcolor_final = if graphics_mode == grmodes_gmMcgaVga as u8 { 0xB7i16 } else { bgcolor };
        draw_rect(&rect, bgcolor_final as c_int);
        fade_in_2(offscreen_surface, 0x1800);
        current_target_surface = onscreen_surface_;
        let name_ptr = core::ptr::addr_of_mut!(hof[hof_index as usize].name[0]);
        while input_str(
            &rect,
            name_ptr,
            24,
            b"\0".as_ptr() as *const c_char,
            0,
            4,
            color as c_int,
            bgcolor_final as c_int,
        ) <= 0 {}
        restore_peel(peel);
        show_hof_text(
            &hof_rects[hof_index as usize] as *const _ as *mut rect_type,
            -1,
            0,
            core::ptr::addr_of!(hof[hof_index as usize].name[0]) as *const c_char,
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

// seg001:0C94
#[no_mangle]
pub unsafe extern "C" fn expired() {
    if demo_mode == 0 {
        if !offscreen_surface.is_null() { free_surface(offscreen_surface); }
        offscreen_surface = core::ptr::null_mut();
        clear_screen_and_sounds();
        offscreen_surface = make_offscreen_buffer(&screen_rect);
        load_intro(1, Some(time_expired), 1);
    }
    start_level = -1;
    start_game();
}

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
    free_all_chtabs_from(chtabs_id_chtab_9_princessbed as c_int);
    let img0 = get_image(chtabs_id_chtab_8_princessroom as c_short, 0);
    SDL_FreeSurface(img0);
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

// seg001:0E1C
#[no_mangle]
pub unsafe extern "C" fn draw_star(which_star: c_int, mark_dirty: c_int) {
    let mut rect = rect_type {
        top: 0, left: 0, bottom: 0, right: 0,
    };
    let star_color: c_int;
    rect.left = stars[which_star as usize].x;
    rect.right = rect.left + 1;
    rect.top = stars[which_star as usize].y;
    rect.bottom = rect.top + 1;
    if graphics_mode != grmodes_gmCga as u8 && graphics_mode != grmodes_gmHgaHerc as u8 {
        stars[which_star as usize].color =
            (stars[which_star as usize].color + 1) % N_STAR_COLORS as c_short;
        star_color = star_colors[stars[which_star as usize].color as usize] as c_int;
    } else {
        star_color = 15;
    }
    draw_rect(&rect, star_color);
    if mark_dirty != 0 {
        add_drect(&mut rect);
    }
}

// seg001:0E94
#[no_mangle]
pub unsafe extern "C" fn show_hof() {
    for index in 0..hof_count as usize {
        let hof_min = core::ptr::addr_of!(hof[index].min).read_unaligned();
        let hof_tick = core::ptr::addr_of!(hof[index].tick).read_unaligned();
        println!(
            "index = {index}, hof[index].min = {hof_min}, hof[index].tick = {hof_tick}"
        );
        // ALLOW_INFINITE_TIME: handle negative minutes (time ran forward from 0:00)
        let (minutes, seconds) = if hof_min > 0 {
            ((hof_min - 1) as i32, (hof_tick / 12) as i32)
        } else if hof_min == 0 {
            (0, 0)
        } else {
            (((-hof_min) - 1) as i32, ((719 - hof_tick) / 12) as i32)
        };
        let time_text = format!("{minutes}:{seconds:02}");
        let time_text_c = std::ffi::CString::new(time_text).unwrap_or_default();
        let name_ptr = core::ptr::addr_of!(hof[index].name[0]) as *const c_char;
        show_hof_text(
            &hof_rects[index] as *const _ as *mut rect_type,
            -1, 0, name_ptr,
        );
        show_hof_text(
            &hof_rects[index] as *const _ as *mut rect_type,
            1, 0, time_text_c.as_ptr(),
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_hof_path(
    custom_path_buffer: *mut c_char,
    max_len: usize,
) -> *const c_char {
    get_writable_file_path(custom_path_buffer, max_len, hof_file.as_ptr() as *const c_char)
}

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

// seg001:0FC3
#[no_mangle]
pub unsafe extern "C" fn show_hof_text(
    rect: *mut rect_type,
    x_align: c_int,
    y_align: c_int,
    text: *const c_char,
) {
    let mut rect2 = rect_type { top: 0, left: 0, bottom: 0, right: 0 };
    let text_color: c_int;
    let shadow_color: c_int = 0;
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        text_color = 0xB7;
    } else {
        text_color = 15;
    }
    offset2_rect(&mut rect2, rect, 1, 1);
    show_text_with_color(&rect2, x_align, y_align, text, shadow_color);
    show_text_with_color(rect as *const rect_type, x_align, y_align, text, text_color);
}

// seg001:1029
#[no_mangle]
pub unsafe extern "C" fn fade_in_1() -> c_int {
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        fade_palette_buffer = make_pal_buffer_fadein(offscreen_surface, 0x6689, 2);
        is_global_fading = 1;
        loop {
            let interrupted = proc_cutscene_frame(1);
            if interrupted == 1 { return 1; }
            if interrupted != 0 { break; }
        }
        is_global_fading = 0;
    } else {
        method_1_blit_rect(onscreen_surface_, offscreen_surface, &screen_rect, &screen_rect, 0);
        update_screen();
    }
    0
}

// seg001:112D
#[no_mangle]
pub unsafe extern "C" fn fade_out_1() -> c_int {
    if graphics_mode == grmodes_gmMcgaVga as u8 {
        fade_palette_buffer = make_pal_buffer_fadeout(0x6689, 2);
        is_global_fading = 1;
        loop {
            let interrupted = proc_cutscene_frame(1);
            if interrupted == 1 { return 1; }
            if interrupted != 0 { break; }
        }
        is_global_fading = 0;
    }
    0
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
}
