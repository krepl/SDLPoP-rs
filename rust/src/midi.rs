// MIDI playback routines — ported from src/midi.c.
// Whole file is inside #ifdef USE_MIDI in C; included unconditionally here.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_void};
use core::ptr::{addr_of_mut, null_mut};
use super::*;

// ============================================================================
// Externs not present in bindings.rs (SDL audio, OPL3 emulator, libc).
// bindgen only processes common.h, which does not include opl3.h.
// ============================================================================
extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn calloc(nmemb: usize, size: usize) -> *mut c_void;
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn printf(fmt: *const c_char, ...) -> c_int;

    fn SDL_LockAudio();
    fn SDL_UnlockAudio();
    fn SDL_PauseAudio(pause_on: c_int);

    // opl3_chip is opaque to us; we only pass a pointer to correctly-sized storage.
    fn OPL3_Reset(chip: *mut c_void, samplerate: u32);
    fn OPL3_WriteReg(chip: *mut c_void, reg: u16, v: u8);
    fn OPL3_GenerateStream(chip: *mut c_void, sndptr: *mut i16, numsamples: u32);
}

// ============================================================================
// Local struct definitions mirroring types.h exactly (avoids depending on
// bindgen's anonymous-union field names, which are not stable to look up here).
// Packed types are inside #pragma pack(push,1) in types.h.
// ============================================================================

#[repr(C, packed)]
struct MidiRawChunk {
    chunk_type: [u8; 4],   // offset 0
    chunk_length: u32,     // offset 4
    format: u16,           // offset 8  (header.format) ; data[] also at offset 8
    num_tracks: u16,       // offset 10 (header.num_tracks)
    time_division: u16,    // offset 12 (header.time_division)
    // header.tracks[] at offset 14
}
const MIDI_CHUNK_DATA_OFFSET: usize = 8; // union { header / byte data[0] } starts here
const MIDI_CHUNK_HEADER_TRACKS_OFFSET: usize = 14; // header.tracks[0]

#[repr(C)]
#[derive(Clone, Copy)]
struct ChannelEvent {
    channel: u8,
    param1: u8,
    param2: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SysexEvent {
    length: u32,
    data: *mut u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MetaEvent {
    type_: u8,
    length: u32,
    data: *mut u8,
}
#[repr(C)]
union MidiEventBody {
    channel: ChannelEvent,
    sysex: SysexEvent,
    meta: MetaEvent,
}
#[repr(C)]
struct MidiEvent {
    delta_time: u32,
    event_type: u8,
    body: MidiEventBody,
}

#[repr(C)]
struct MidiTrack {
    size: u32,
    num_events: c_int,
    events: *mut MidiEvent,
    event_index: c_int,
    next_pause_tick: i64,
}

#[repr(C)]
struct ParsedMidi {
    num_tracks: c_int,
    tracks: *mut MidiTrack,
    ticks_per_beat: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OperatorT {
    mul: u8,
    ksl_tl: u8,
    a_d: u8,
    s_r: u8,
    waveform: u8,
}
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct InstrumentT {
    blocknum_low: u8,
    blocknum_high: u8,
    FB_conn: u8,
    operators: [OperatorT; 2],
    percussion: u8,
    unknown: [u8; 2],
}

// seg009's SDL_AudioSpec has private fields; replicate the layout here so we can
// read `freq` through a pointer cast (freq is the first field, offset 0).
#[repr(C)]
struct AudioSpec {
    freq: c_int,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<unsafe extern "C" fn(*mut c_void, *mut u8, c_int)>,
    userdata: *mut c_void,
}

// ============================================================================
// Constants
// ============================================================================
const MAX_MIDI_CHANNELS: usize = 16;
const MAX_OPL_VOICES: usize = 18;
const NUM_OPL_VOICES: c_int = 9;

// opl3_chip on x86-64 is ~20776 bytes; this storage is generously sized and
// 8-byte aligned. OPL3_* only touch sizeof(opl3_chip) bytes from the start.
const OPL3_CHIP_SIZE: usize = 32768;
#[repr(C, align(8))]
struct OplChipStorage([u8; OPL3_CHIP_SIZE]);

// ============================================================================
// File-scope statics (static in C — not in data.h)
// ============================================================================
static mut opl_chip: OplChipStorage = OplChipStorage([0u8; OPL3_CHIP_SIZE]);
static mut instruments_data: *mut c_void = null_mut();
static mut instruments: *mut InstrumentT = null_mut();
static mut num_instruments: c_int = 0;
static mut voice_note: [u8; MAX_OPL_VOICES] = [0; MAX_OPL_VOICES];
static mut voice_instrument: [c_int; MAX_OPL_VOICES] = [0; MAX_OPL_VOICES];
static mut voice_channel: [c_int; MAX_OPL_VOICES] = [0; MAX_OPL_VOICES];
static mut channel_instrument: [c_int; MAX_MIDI_CHANNELS] = [0; MAX_MIDI_CHANNELS];
static mut last_used_voice: c_int = 0;
static mut num_midi_tracks: c_int = 0;
static mut parsed_midi: ParsedMidi = ParsedMidi { num_tracks: 0, tracks: null_mut(), ticks_per_beat: 0 };
static mut midi_tracks: *mut MidiTrack = null_mut();
static mut midi_current_pos: i64 = 0; // in MIDI ticks
static mut midi_current_pos_fract_part: f32 = 0.0; // partial ticks after the decimal point
static mut ticks_to_next_pause: c_int = 0; // in MIDI ticks
static mut us_per_beat: u32 = 0;
static mut ticks_per_beat: u32 = 0;
static mut mixing_freq: c_int = 0;
static mut midi_semitones_higher: i8 = 0;
static mut current_midi_tempo_modifier: f32 = 0.0;

// Tempo adjustments for specific songs:
//   index 53 = sound_53_story_3_Jaffar_comes  -> -0.03 (3% speedup)
//   index 54 = sound_54_intro_music           ->  0.03 (3% slowdown)
static midi_tempo_modifiers: [f32; 58] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // 0-9
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // 10-19
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // 20-29
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // 30-39
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // 40-49
    0.0, 0.0, 0.0,                                      // 50-52
    -0.03,                                              // 53 sound_53_story_3_Jaffar_comes
    0.03,                                               // 54 sound_54_intro_music
    0.0, 0.0, 0.0,                                      // 55-57
];

// The hardcoded instrument is used as a fallback, if instrument data is not available.
static mut hardcoded_instrument: InstrumentT = InstrumentT {
    blocknum_low: 0x13,
    blocknum_high: 0x09,
    FB_conn: 0x04,
    operators: [
        OperatorT { mul: 0x02, ksl_tl: 0x8D, a_d: 0xD7, s_r: 0x37, waveform: 0x00 },
        OperatorT { mul: 0x03, ksl_tl: 0x03, a_d: 0xF5, s_r: 0x18, waveform: 0x00 },
    ],
    percussion: 0x00,
    unknown: [0x00, 0x00],
};

static mut opl_cached_regs: [u8; 512] = [0; 512];

// init_midi's function-static `initialized`
static mut INIT_MIDI_INITIALIZED: bool = false;

// Reference: https://www.fit.vutbr.cz/~arnost/opl/opl3.html#appendixA
static sbpro_op: [u8; 18] = [0, 1, 2, 6, 7, 8, 12, 13, 14, 18, 19, 20, 24, 25, 26, 30, 31, 32];

static reg_pair_offsets: [u16; 36] = [
    0x000, 0x001, 0x002, 0x003, 0x004, 0x005,
    0x008, 0x009, 0x00A, 0x00B, 0x00C, 0x00D,
    0x010, 0x011, 0x012, 0x013, 0x014, 0x015,
    0x100, 0x101, 0x102, 0x103, 0x104, 0x105,
    0x108, 0x109, 0x10A, 0x10B, 0x10C, 0x10D,
    0x110, 0x111, 0x112, 0x113, 0x114, 0x115,
];

static reg_single_offsets: [u16; 18] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0x100, 0x101, 0x102, 0x103, 0x104, 0x105, 0x106, 0x107, 0x108,
];

// ============================================================================
// Read a variable length integer (max 4 bytes).
// ============================================================================
unsafe fn midi_read_variable_length(buffer_position: &mut *mut u8) -> u32 {
    let mut result: u32 = 0;
    let pos = *buffer_position;
    let mut i: c_int = 0;
    while i < 4 {
        result = (result << 7) | (*pos.add(i as usize) & 0x7F) as u32;
        if (*pos.add(i as usize) & 0x80) == 0 {
            break; // The most significant bit being 0 means that this is the last byte.
        }
        i += 1;
    }
    *buffer_position = (*buffer_position).add((i + 1) as usize); // Advance the pointer.
    result
}

#[no_mangle]
unsafe extern "C" fn free_parsed_midi(mut pm: *mut ParsedMidi) {
    let mut i: c_int = 0;
    while i < (*pm).num_tracks {
        free((*(*pm).tracks.add(i as usize)).events as *mut c_void);
        i += 1;
    }
    free((*pm).tracks as *mut c_void);
    // Faithful reproduction of the original (harmless) bug: memset zeroes the
    // local pointer variable, not the pointee (sizeof(parsed_midi) == 8).
    memset(addr_of_mut!(pm) as *mut c_void, 0, core::mem::size_of::<*mut ParsedMidi>());
}

unsafe fn parse_midi(midi: *mut MidiRawChunk, mut pm: *mut ParsedMidi) -> bool {
    (*pm).ticks_per_beat = 24;
    {
        let ct = (*midi).chunk_type;
        if ct != *b"MThd" {
            printf(b"Warning: Tried to play a midi sound without the 'MThd' chunk header.\n\0".as_ptr() as *const c_char);
            return false;
        }
    }
    if u32::from_be((*midi).chunk_length) != 6 {
        printf(
            b"Warning: Midi file with an invalid header length (expected 6, is %d)\n\0".as_ptr() as *const c_char,
            u32::from_be((*midi).chunk_length) as c_int,
        );
        return false;
    }
    let midi_format: u16 = u16::from_be((*midi).format);
    if midi_format >= 2 {
        printf(
            b"Warning: Unsupported midi format %d (only type 0 or 1 files are supported)\n\0".as_ptr() as *const c_char,
            midi_format as c_int,
        );
        return false;
    }
    let num_tracks: u16 = u16::from_be((*midi).num_tracks);
    if num_tracks < 1 {
        printf(b"Warning: Midi sound does not have any tracks.\n\0".as_ptr() as *const c_char);
        return false;
    }
    let mut division: c_int = u16::from_be((*midi).time_division) as c_int;
    if division < 0 {
        division = (-(division / 256)) * (division & 0xFF); // Translate time delta from the alternative SMTPE format.
    }
    (*pm).ticks_per_beat = division as u32;

    (*pm).tracks = calloc(1, (num_tracks as usize) * core::mem::size_of::<MidiTrack>()) as *mut MidiTrack;
    (*pm).num_tracks = num_tracks as c_int;
    // The first track chunk starts after the header chunk.
    let mut next_track_chunk: *mut MidiRawChunk =
        (midi as *mut u8).add(MIDI_CHUNK_HEADER_TRACKS_OFFSET) as *mut MidiRawChunk;
    let mut last_event_type: u8 = 0;
    let mut track_index: c_int = 0;
    while track_index < num_tracks as c_int {
        let track_chunk: *mut MidiRawChunk = next_track_chunk;
        {
            let ct = (*track_chunk).chunk_type;
            if ct != *b"MTrk" {
                printf(b"Warning: midi track without 'MTrk' chunk header.\n\0".as_ptr() as *const c_char);
                free((*pm).tracks as *mut c_void);
                memset(addr_of_mut!(pm) as *mut c_void, 0, core::mem::size_of::<*mut ParsedMidi>());
                return false;
            }
        }
        next_track_chunk = ((track_chunk as *mut u8).add(MIDI_CHUNK_DATA_OFFSET))
            .add(u32::from_be((*track_chunk).chunk_length) as usize) as *mut MidiRawChunk;
        let track: *mut MidiTrack = (*pm).tracks.add(track_index as usize);
        let mut buffer_position: *mut u8 = (track_chunk as *mut u8).add(MIDI_CHUNK_DATA_OFFSET);
        loop {
            (*track).num_events += 1;
            let new_track_events =
                realloc((*track).events as *mut c_void, (*track).num_events as usize * core::mem::size_of::<MidiEvent>());
            if new_track_events.is_null() {
                printf(b"parse_midi: realloc failed!\0".as_ptr() as *const c_char);
                quit(1);
            }
            (*track).events = new_track_events as *mut MidiEvent;

            let event: *mut MidiEvent = (*track).events.add(((*track).num_events - 1) as usize);
            (*event).delta_time = midi_read_variable_length(&mut buffer_position);
            (*event).event_type = *buffer_position;
            if ((*event).event_type & 0x80) != 0 {
                if (*event).event_type < 0xF8 {
                    last_event_type = (*event).event_type;
                }
                buffer_position = buffer_position.add(1);
            } else {
                (*event).event_type = last_event_type; // Implicit use of the previous event type.
            }
            // Determine the event type and parse the event.
            let masked = (*event).event_type & 0xF0;
            match masked {
                0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 | 0xC0 | 0xD0 => {
                    // Read the channel event.
                    let mut num_channel_event_params: c_int = 1;
                    if matches!(masked, 0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0) {
                        num_channel_event_params = 2;
                    }
                    (*event).body.channel.channel = (*event).event_type & 0x0F;
                    (*event).event_type &= 0xF0;
                    (*event).body.channel.param1 = *buffer_position;
                    buffer_position = buffer_position.add(1);
                    if num_channel_event_params == 2 {
                        (*event).body.channel.param2 = *buffer_position;
                        buffer_position = buffer_position.add(1);
                    }
                }
                _ => {
                    // Not a channel event.
                    match (*event).event_type {
                        0xF0 | 0xF7 => {
                            // Read SysEx event
                            (*event).body.sysex.length = midi_read_variable_length(&mut buffer_position);
                            (*event).body.sysex.data = buffer_position;
                            buffer_position = buffer_position.add((*event).body.sysex.length as usize);
                        }
                        0xFF => {
                            // Meta event
                            (*event).body.meta.type_ = *buffer_position;
                            buffer_position = buffer_position.add(1);
                            (*event).body.meta.length = midi_read_variable_length(&mut buffer_position);
                            (*event).body.meta.data = buffer_position;
                            buffer_position = buffer_position.add((*event).body.meta.length as usize);
                        }
                        _ => {
                            printf(
                                b"Warning: unknown midi event type 0x%02x (track %d, event %d)\n\0".as_ptr() as *const c_char,
                                (*event).event_type as c_int,
                                track_index,
                                (*track).num_events - 1,
                            );
                            free_parsed_midi(pm);
                            return false;
                        }
                    }
                }
            }
            if (*event).event_type == 0xFF /* meta event */ && (*event).body.meta.type_ == 0x2F /* end of track */ {
                break;
            }
            if buffer_position >= next_track_chunk as *mut u8 {
                printf(
                    b"Error parsing MIDI events (track %d)\n\0".as_ptr() as *const c_char,
                    track_index,
                );
                free_parsed_midi(pm);
                return false;
            }
        }
        track_index += 1;
    }

    true
}

unsafe fn opl_reset(freq: c_int) {
    OPL3_Reset(addr_of_mut!(opl_chip) as *mut c_void, freq as u32);
    memset(addr_of_mut!(opl_cached_regs) as *mut c_void, 0, 512);
}

unsafe fn opl_write_reg(reg: u16, value: u8) {
    OPL3_WriteReg(addr_of_mut!(opl_chip) as *mut c_void, reg, value);
    opl_cached_regs[reg as usize] = value;
}

unsafe fn opl_write_reg_masked(reg: u16, value: u8, mask: u8) {
    let cached = opl_cached_regs[reg as usize] & !mask;
    let value = cached | (value & mask);
    opl_write_reg(reg, value);
}

unsafe fn opl_reg_pair_offset(voice: u8, op: u8) -> u16 {
    let mut reg_offset = reg_pair_offsets[sbpro_op[voice as usize] as usize];
    if op == 1 {
        reg_offset += 3;
    }
    reg_offset
}

unsafe fn opl_write_instrument(instrument: *mut InstrumentT, voice: u8) {
    let instr = *instrument;
    opl_write_reg(
        0xC0u16 + reg_single_offsets[voice as usize],
        instr.FB_conn | 0x30, /* OPL3: L+R speaker enable */
    );
    let operators = instr.operators;
    let mut operator_index: u8 = 0;
    while operator_index < 2 {
        let op = operators[operator_index as usize];
        let op_reg = opl_reg_pair_offset(voice, operator_index);
        opl_write_reg(0x20u16 + op_reg, op.mul);
        opl_write_reg(0x40u16 + op_reg, op.ksl_tl);
        opl_write_reg(0x60u16 + op_reg, op.a_d);
        opl_write_reg(0x80u16 + op_reg, op.s_r);
        opl_write_reg(0xE0u16 + op_reg, op.waveform);
        operator_index += 1;
    }
}

unsafe fn midi_note_off(event: *mut MidiEvent) {
    let note: u8 = (*event).body.channel.param1;
    let channel: u8 = (*event).body.channel.channel;
    let mut voice: c_int = 0;
    while voice < NUM_OPL_VOICES {
        if voice_channel[voice as usize] == channel as c_int && voice_note[voice as usize] == note {
            opl_write_reg_masked(0xB0u16 + reg_single_offsets[voice as usize], 0, 0x20); // release key
            voice_note[voice as usize] = 0; // This voice is now free to be re-used.
            break;
        }
        voice += 1;
    }
}

unsafe fn get_instrument(id: c_int) -> *mut InstrumentT {
    if id >= 0 && id < num_instruments {
        instruments.add(id as usize)
    } else {
        instruments
    }
}

unsafe fn midi_note_on(event: *mut MidiEvent) {
    let note: u8 = (*event).body.channel.param1;
    let velocity: u8 = (*event).body.channel.param2;
    let channel: u8 = (*event).body.channel.channel;
    let instrument_id: c_int = channel_instrument[channel as usize];
    let instrument: *mut InstrumentT = get_instrument(instrument_id);

    if velocity == 0 {
        midi_note_off(event);
    } else {
        // Find a free OPL voice.
        let mut voice: c_int = -1;
        let mut test_voice: c_int = last_used_voice;
        let mut i: c_int = 0;
        while i < NUM_OPL_VOICES {
            // Don't use the same voice immediately again: that note is probably still in the release phase.
            test_voice += 1;
            test_voice %= NUM_OPL_VOICES;
            if voice_note[test_voice as usize] == 0 {
                voice = test_voice;
                break;
            }
            i += 1;
        }
        last_used_voice = voice;
        if voice >= 0 {
            let instr = *instrument;
            let ops = instr.operators;

            // Set the correct instrument for this voice.
            if voice_instrument[voice as usize] != instrument_id {
                opl_write_instrument(instrument, voice as u8);
                voice_instrument[voice as usize] = instrument_id;
            }
            voice_note[voice as usize] = note;
            voice_channel[voice as usize] = channel as c_int;

            // Calculate frequency for a MIDI note: note number 69 = A4 = 440 Hz.
            let octaves_from_A4: f32 =
                (((*event).body.channel.param1 as c_int) - 69 - 12 + midi_semitones_higher as c_int) as f32 / 12.0f32;
            let frequency: f32 = 2.0f32.powf(octaves_from_A4) * 440.0f32;
            let f_number_float: f32 = frequency * (1048576.0f32) / 49716.0f32; // 1<<20
            let block: c_int = ((f_number_float.log2() - 9.0f32) as c_int) & 7;
            let f: c_int = ((f_number_float as c_int) >> block) & 1023;
            let reg_offset: u16 = reg_single_offsets[voice as usize];
            opl_write_reg(0xA0u16 + reg_offset, (f & 0xFF) as u8);
            opl_write_reg(0xB0u16 + reg_offset, (0x20 | (block << 2) | (f >> 8)) as u8);

            // The modulator always uses its own base volume level.
            opl_write_reg_masked(0x40u16 + opl_reg_pair_offset(voice as u8, 0), ops[0].ksl_tl, 0x3F);

            // The carrier volume level is a combination of its base volume and the MIDI note velocity.
            let instr_volume: c_int = (ops[1].ksl_tl & 0x3F) as c_int;
            let mut carrier_volume: c_int = ((instr_volume + 64) * 225) / (velocity as c_int + 161);
            if carrier_volume < 64 {
                carrier_volume = 64;
            }
            if carrier_volume > 127 {
                carrier_volume = 127;
            }
            carrier_volume -= 64;
            opl_write_reg_masked(0x40u16 + opl_reg_pair_offset(voice as u8, 1), carrier_volume as u8, 0x3F);
        } else {
            printf(b"skipping note, not enough OPL voices\n\0".as_ptr() as *const c_char);
        }
    }
}

unsafe fn process_midi_event(event: *mut MidiEvent) {
    match (*event).event_type {
        0x80 => {
            // note off
            midi_note_off(event);
        }
        0x90 => {
            // note on
            midi_note_on(event);
        }
        0xC0 => {
            // program change
            channel_instrument[(*event).body.channel.channel as usize] = (*event).body.channel.param1 as c_int;
        }
        0xF0 => {
            // SysEx event:
            if (*event).body.sysex.length == 7 {
                let data = (*event).body.sysex.data;
                if *data.add(2) == 0x34 && (*data.add(3) == 0 || *data.add(3) == 1) && *data.add(4) == 0 {
                    midi_semitones_higher = *data.add(5) as i8; // Make all notes higher by this amount.
                }
            }
        }
        0xFF => {
            // Meta event
            match (*event).body.meta.type_ {
                0x51 => {
                    // set tempo
                    let data = (*event).body.meta.data;
                    let mut new_tempo: c_int =
                        ((*data.add(0) as c_int) << 16) | ((*data.add(1) as c_int) << 8) | (*data.add(2) as c_int);
                    new_tempo = (new_tempo as f32 * (1.0f32 + current_midi_tempo_modifier)) as c_int; // tempo adjustment
                    us_per_beat = new_tempo as u32;
                }
                0x54 => {} // SMTPE offset
                0x58 => {} // time signature
                0x2F => {} // end of track
                _ => {}
            }
        }
        _ => {}
    }
}

const ONE_SECOND_IN_US: i64 = 1000000;

#[no_mangle]
pub unsafe extern "C" fn midi_callback(_userdata: *mut c_void, stream: *mut u8, len: c_int) {
    if crate::seg009::midi_playing == 0 || len <= 0 {
        return;
    }
    let mut stream = stream;
    let mut frames_needed: c_int = len / 4;
    while frames_needed > 0 {
        if ticks_to_next_pause > 0 {
            // Fill the audio buffer (events already processed up till this point).
            let us_to_next_pause: i64 =
                ((ticks_to_next_pause as u32).wrapping_mul(us_per_beat) / ticks_per_beat) as i64;
            let us_needed: i64 = (frames_needed as i64 * ONE_SECOND_IN_US) / mixing_freq as i64;
            let mut advance_us: i64 = us_to_next_pause.min(us_needed);
            // round up.
            let available_frames: c_int =
                (((advance_us * mixing_freq as i64) + ONE_SECOND_IN_US - 1) / ONE_SECOND_IN_US) as c_int;
            let advance_frames: c_int = available_frames.min(frames_needed);
            advance_us = (advance_frames as i64 * ONE_SECOND_IN_US) / mixing_freq as i64; // recalculate
            let temp_buffer = malloc((advance_frames * 4) as usize) as *mut i16;
            OPL3_GenerateStream(addr_of_mut!(opl_chip) as *mut c_void, temp_buffer, advance_frames as u32);
            if is_sound_on != 0 && enable_music != 0 {
                let mut sample: c_int = 0;
                while sample < advance_frames * 2 {
                    let dst = (stream as *mut i16).add(sample as usize);
                    *dst = (*dst).wrapping_add(*temp_buffer.add(sample as usize));
                    sample += 1;
                }
            }
            free(temp_buffer as *mut c_void);

            frames_needed -= advance_frames;
            stream = stream.add((advance_frames * 4) as usize);
            // Advance the current MIDI tick position; track partial ticks so we don't fall behind.
            let ticks_elapsed_float: f32 = (advance_us as f32) * (ticks_per_beat as f32) / (us_per_beat as f32);
            let mut ticks_elapsed: i64 = ticks_elapsed_float as i64;
            midi_current_pos_fract_part += ticks_elapsed_float - ticks_elapsed as f32;
            if midi_current_pos_fract_part > 1.0f32 {
                midi_current_pos_fract_part -= 1.0f32;
                ticks_elapsed += 1;
            }
            midi_current_pos += ticks_elapsed;
            ticks_to_next_pause = (ticks_to_next_pause as i64 - ticks_elapsed) as c_int;
        } else {
            // Need to process MIDI events on one or more tracks.
            let mut num_finished_tracks: c_int = 0;
            let mut track_index: c_int = 0;
            while track_index < num_midi_tracks {
                let track: *mut MidiTrack = midi_tracks.add(track_index as usize);

                while midi_current_pos >= (*track).next_pause_tick {
                    let events_left: c_int = (*track).num_events - (*track).event_index;
                    if events_left > 0 {
                        let event: *mut MidiEvent = (*track).events.add((*track).event_index as usize);
                        (*track).event_index += 1;
                        process_midi_event(event);

                        // Look ahead: delay processing of the next event if there is a pause.
                        if events_left > 1 {
                            let next_event: *mut MidiEvent = (*track).events.add((*track).event_index as usize);
                            if (*next_event).delta_time != 0 {
                                (*track).next_pause_tick += (*next_event).delta_time as i64;
                            }
                        }
                    } else {
                        // reached the last event in this track.
                        num_finished_tracks += 1;
                        break;
                    }
                }
                track_index += 1;
            }
            if num_finished_tracks >= num_midi_tracks {
                // All tracks have finished. Fill the remaining samples with silence and stop playback.
                memset(stream as *mut c_void, 0, (frames_needed * 4) as usize);
                SDL_LockAudio();
                crate::seg009::midi_playing = 0;
                free_parsed_midi(addr_of_mut!(parsed_midi));
                SDL_UnlockAudio();
                return;
            } else {
                // Delay (let the OPL chip work) until a track needs to process a MIDI event again.
                let mut first_next_pause_tick: i64 = i64::MAX;
                let mut i: c_int = 0;
                while i < num_midi_tracks {
                    let track: *mut MidiTrack = midi_tracks.add(i as usize);
                    if (*track).event_index >= (*track).num_events || midi_current_pos >= (*track).next_pause_tick {
                        i += 1;
                        continue;
                    }
                    first_next_pause_tick = first_next_pause_tick.min((*track).next_pause_tick);
                    i += 1;
                }
                if first_next_pause_tick == i64::MAX {
                    printf(b"MIDI: Couldn't figure out how long to delay (this is a bug)\n\0".as_ptr() as *const c_char);
                    quit(1);
                }
                ticks_to_next_pause = (first_next_pause_tick - midi_current_pos) as c_int;
                if ticks_to_next_pause < 0 {
                    printf(b"Tried to delay a negative amount of time (this is a bug)\n\0".as_ptr() as *const c_char);
                    quit(1);
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stop_midi() {
    if crate::seg009::midi_playing == 0 {
        return;
    }
    SDL_LockAudio();
    crate::seg009::midi_playing = 0;
    free_parsed_midi(addr_of_mut!(parsed_midi));
    SDL_UnlockAudio();
}

#[no_mangle]
pub unsafe extern "C" fn free_midi_resources() {
    free(instruments_data);
}

#[no_mangle]
pub unsafe extern "C" fn init_midi() {
    if INIT_MIDI_INITIALIZED {
        return;
    }
    INIT_MIDI_INITIALIZED = true;

    instruments = addr_of_mut!(hardcoded_instrument); // unused if instruments can be loaded normally.
    let mut size: c_int = 0;
    let dathandle: *mut dat_type = open_dat(b"PRINCE.DAT\0".as_ptr() as *const c_char, 0);
    instruments_data = load_from_opendats_alloc(1, b"bin\0".as_ptr() as *const c_char, null_mut(), &mut size);
    if instruments_data.is_null() {
        printf(b"Missing MIDI instruments data (resource 1)\n\0".as_ptr() as *const c_char);
    } else {
        num_instruments = *(instruments_data as *mut u8) as c_int;
        if size == 1 + num_instruments * core::mem::size_of::<InstrumentT>() as c_int {
            instruments = (instruments_data as *mut u8).add(1) as *mut InstrumentT;
        } else {
            printf(b"MIDI instruments data (resource 1) is not the expected size\n\0".as_ptr() as *const c_char);
            num_instruments = 1;
        }
    }
    if !dathandle.is_null() {
        close_dat(dathandle);
    }
}

#[no_mangle]
pub unsafe extern "C" fn play_midi_sound(buffer: *mut sound_buffer_type) {
    stop_midi();
    if buffer.is_null() {
        return;
    }
    init_digi();
    if crate::seg009::digi_unavailable != 0 {
        return;
    }
    init_midi();

    // (midi_raw_chunk_type*) &buffer->midi : sound_buffer_type is packed { byte type; union {...}; }
    let midi: *mut MidiRawChunk = (buffer as *mut u8).add(1) as *mut MidiRawChunk;
    if !parse_midi(midi, addr_of_mut!(parsed_midi)) {
        printf(b"Error reading MIDI music\n\0".as_ptr() as *const c_char);
        return;
    }

    // Initialize the OPL chip.
    opl_reset((*(crate::seg009::digi_audiospec as *const AudioSpec)).freq);
    opl_write_reg(0x105, 0x01); // OPL3 enable
    let mut voice: c_int = 0;
    while voice < NUM_OPL_VOICES {
        opl_write_instrument(instruments, voice as u8);
        voice_instrument[voice as usize] = 0;
        voice_note[voice as usize] = 0;
        voice += 1;
    }
    let mut channel: c_int = 0;
    while channel < MAX_MIDI_CHANNELS as c_int {
        channel_instrument[channel as usize] = channel;
        channel += 1;
    }

    midi_current_pos = 0;
    midi_current_pos_fract_part = 0.0;
    ticks_to_next_pause = 0;
    midi_tracks = parsed_midi.tracks;
    num_midi_tracks = parsed_midi.num_tracks;
    midi_semitones_higher = 0;
    us_per_beat = 500000; // default tempo (500000 us/beat == 120 bpm)
    current_midi_tempo_modifier = midi_tempo_modifiers[current_sound as usize];
    ticks_per_beat = parsed_midi.ticks_per_beat;
    mixing_freq = (*(crate::seg009::digi_audiospec as *const AudioSpec)).freq;
    crate::seg009::midi_playing = 1;
    SDL_PauseAudio(0);
}
