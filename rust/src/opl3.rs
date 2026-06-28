// Port of src/opl3.c — Nuked OPL3 emulator (version 1.7.4)
// Faithful, block-by-block translation. This unit is standalone: it touches no
// game globals and is called from the still-C midi.c, which only passes
// `&opl_chip`. The opl3 structs are NOT in bindings.rs (opl3.h is not included by
// common.h), so they are defined here with #[repr(C)] to match opl3.h layout.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

const RSM_FRAC: u32 = 10;

const OPL_WRITEBUF_SIZE: usize = 1024;
const OPL_WRITEBUF_DELAY: u64 = 2;

// Channel types
const ch_2op: u8 = 0;
const ch_4op: u8 = 1;
const ch_4op2: u8 = 2;
const ch_drum: u8 = 3;

// Envelope key types
const egk_norm: u8 = 0x01;
const egk_drum: u8 = 0x02;

// Envelope generator state numbers
const envelope_gen_num_off: u8 = 0;
const envelope_gen_num_attack: u8 = 1;
const envelope_gen_num_decay: u8 = 2;
const envelope_gen_num_sustain: u8 = 3;
const envelope_gen_num_release: u8 = 4;

//
// Struct definitions (mirror opl3.h exactly)
//

#[repr(C)]
pub struct opl3_slot {
    pub channel: *mut opl3_channel,
    pub chip: *mut opl3_chip,
    pub out: i16,
    pub fbmod: i16,
    pub mod_: *mut i16,
    pub prout: i16,
    pub eg_rout: i16,
    pub eg_out: i16,
    pub eg_inc: u8,
    pub eg_gen: u8,
    pub eg_rate: u8,
    pub eg_ksl: u8,
    pub trem: *mut u8,
    pub reg_vib: u8,
    pub reg_type: u8,
    pub reg_ksr: u8,
    pub reg_mult: u8,
    pub reg_ksl: u8,
    pub reg_tl: u8,
    pub reg_ar: u8,
    pub reg_dr: u8,
    pub reg_sl: u8,
    pub reg_rr: u8,
    pub reg_wf: u8,
    pub key: u8,
    pub pg_phase: u32,
    pub timer: u32,
}

#[repr(C)]
pub struct opl3_channel {
    pub slots: [*mut opl3_slot; 2],
    pub pair: *mut opl3_channel,
    pub chip: *mut opl3_chip,
    pub out: [*mut i16; 4],
    pub chtype: u8,
    pub f_num: u16,
    pub block: u8,
    pub fb: u8,
    pub con: u8,
    pub alg: u8,
    pub ksv: u8,
    pub cha: u16,
    pub chb: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct opl3_writebuf {
    pub time: u64,
    pub reg: u16,
    pub data: u8,
}

#[repr(C)]
pub struct opl3_chip {
    pub channel: [opl3_channel; 18],
    pub slot: [opl3_slot; 36],
    pub timer: u16,
    pub newm: u8,
    pub nts: u8,
    pub rhy: u8,
    pub vibpos: u8,
    pub vibshift: u8,
    pub tremolo: u8,
    pub tremolopos: u8,
    pub tremoloshift: u8,
    pub noise: u32,
    pub zeromod: i16,
    pub mixbuff: [i32; 2],
    // OPL3L
    pub rateratio: i32,
    pub samplecnt: i32,
    pub oldsamples: [i16; 2],
    pub samples: [i16; 2],

    pub writebuf_samplecnt: u64,
    pub writebuf_cur: u32,
    pub writebuf_last: u32,
    pub writebuf_lasttime: u64,
    pub writebuf: [opl3_writebuf; OPL_WRITEBUF_SIZE],
}

//
// logsin table
//
static logsinrom: [u16; 256] = [
    0x859, 0x6c3, 0x607, 0x58b, 0x52e, 0x4e4, 0x4a6, 0x471,
    0x443, 0x41a, 0x3f5, 0x3d3, 0x3b5, 0x398, 0x37e, 0x365,
    0x34e, 0x339, 0x324, 0x311, 0x2ff, 0x2ed, 0x2dc, 0x2cd,
    0x2bd, 0x2af, 0x2a0, 0x293, 0x286, 0x279, 0x26d, 0x261,
    0x256, 0x24b, 0x240, 0x236, 0x22c, 0x222, 0x218, 0x20f,
    0x206, 0x1fd, 0x1f5, 0x1ec, 0x1e4, 0x1dc, 0x1d4, 0x1cd,
    0x1c5, 0x1be, 0x1b7, 0x1b0, 0x1a9, 0x1a2, 0x19b, 0x195,
    0x18f, 0x188, 0x182, 0x17c, 0x177, 0x171, 0x16b, 0x166,
    0x160, 0x15b, 0x155, 0x150, 0x14b, 0x146, 0x141, 0x13c,
    0x137, 0x133, 0x12e, 0x129, 0x125, 0x121, 0x11c, 0x118,
    0x114, 0x10f, 0x10b, 0x107, 0x103, 0x0ff, 0x0fb, 0x0f8,
    0x0f4, 0x0f0, 0x0ec, 0x0e9, 0x0e5, 0x0e2, 0x0de, 0x0db,
    0x0d7, 0x0d4, 0x0d1, 0x0cd, 0x0ca, 0x0c7, 0x0c4, 0x0c1,
    0x0be, 0x0bb, 0x0b8, 0x0b5, 0x0b2, 0x0af, 0x0ac, 0x0a9,
    0x0a7, 0x0a4, 0x0a1, 0x09f, 0x09c, 0x099, 0x097, 0x094,
    0x092, 0x08f, 0x08d, 0x08a, 0x088, 0x086, 0x083, 0x081,
    0x07f, 0x07d, 0x07a, 0x078, 0x076, 0x074, 0x072, 0x070,
    0x06e, 0x06c, 0x06a, 0x068, 0x066, 0x064, 0x062, 0x060,
    0x05e, 0x05c, 0x05b, 0x059, 0x057, 0x055, 0x053, 0x052,
    0x050, 0x04e, 0x04d, 0x04b, 0x04a, 0x048, 0x046, 0x045,
    0x043, 0x042, 0x040, 0x03f, 0x03e, 0x03c, 0x03b, 0x039,
    0x038, 0x037, 0x035, 0x034, 0x033, 0x031, 0x030, 0x02f,
    0x02e, 0x02d, 0x02b, 0x02a, 0x029, 0x028, 0x027, 0x026,
    0x025, 0x024, 0x023, 0x022, 0x021, 0x020, 0x01f, 0x01e,
    0x01d, 0x01c, 0x01b, 0x01a, 0x019, 0x018, 0x017, 0x017,
    0x016, 0x015, 0x014, 0x014, 0x013, 0x012, 0x011, 0x011,
    0x010, 0x00f, 0x00f, 0x00e, 0x00d, 0x00d, 0x00c, 0x00c,
    0x00b, 0x00a, 0x00a, 0x009, 0x009, 0x008, 0x008, 0x007,
    0x007, 0x007, 0x006, 0x006, 0x005, 0x005, 0x005, 0x004,
    0x004, 0x004, 0x003, 0x003, 0x003, 0x002, 0x002, 0x002,
    0x002, 0x001, 0x001, 0x001, 0x001, 0x001, 0x001, 0x001,
    0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
];

//
// exp table
//
static exprom: [u16; 256] = [
    0x000, 0x003, 0x006, 0x008, 0x00b, 0x00e, 0x011, 0x014,
    0x016, 0x019, 0x01c, 0x01f, 0x022, 0x025, 0x028, 0x02a,
    0x02d, 0x030, 0x033, 0x036, 0x039, 0x03c, 0x03f, 0x042,
    0x045, 0x048, 0x04b, 0x04e, 0x051, 0x054, 0x057, 0x05a,
    0x05d, 0x060, 0x063, 0x066, 0x069, 0x06c, 0x06f, 0x072,
    0x075, 0x078, 0x07b, 0x07e, 0x082, 0x085, 0x088, 0x08b,
    0x08e, 0x091, 0x094, 0x098, 0x09b, 0x09e, 0x0a1, 0x0a4,
    0x0a8, 0x0ab, 0x0ae, 0x0b1, 0x0b5, 0x0b8, 0x0bb, 0x0be,
    0x0c2, 0x0c5, 0x0c8, 0x0cc, 0x0cf, 0x0d2, 0x0d6, 0x0d9,
    0x0dc, 0x0e0, 0x0e3, 0x0e7, 0x0ea, 0x0ed, 0x0f1, 0x0f4,
    0x0f8, 0x0fb, 0x0ff, 0x102, 0x106, 0x109, 0x10c, 0x110,
    0x114, 0x117, 0x11b, 0x11e, 0x122, 0x125, 0x129, 0x12c,
    0x130, 0x134, 0x137, 0x13b, 0x13e, 0x142, 0x146, 0x149,
    0x14d, 0x151, 0x154, 0x158, 0x15c, 0x160, 0x163, 0x167,
    0x16b, 0x16f, 0x172, 0x176, 0x17a, 0x17e, 0x181, 0x185,
    0x189, 0x18d, 0x191, 0x195, 0x199, 0x19c, 0x1a0, 0x1a4,
    0x1a8, 0x1ac, 0x1b0, 0x1b4, 0x1b8, 0x1bc, 0x1c0, 0x1c4,
    0x1c8, 0x1cc, 0x1d0, 0x1d4, 0x1d8, 0x1dc, 0x1e0, 0x1e4,
    0x1e8, 0x1ec, 0x1f0, 0x1f5, 0x1f9, 0x1fd, 0x201, 0x205,
    0x209, 0x20e, 0x212, 0x216, 0x21a, 0x21e, 0x223, 0x227,
    0x22b, 0x230, 0x234, 0x238, 0x23c, 0x241, 0x245, 0x249,
    0x24e, 0x252, 0x257, 0x25b, 0x25f, 0x264, 0x268, 0x26d,
    0x271, 0x276, 0x27a, 0x27f, 0x283, 0x288, 0x28c, 0x291,
    0x295, 0x29a, 0x29e, 0x2a3, 0x2a8, 0x2ac, 0x2b1, 0x2b5,
    0x2ba, 0x2bf, 0x2c4, 0x2c8, 0x2cd, 0x2d2, 0x2d6, 0x2db,
    0x2e0, 0x2e5, 0x2e9, 0x2ee, 0x2f3, 0x2f8, 0x2fd, 0x302,
    0x306, 0x30b, 0x310, 0x315, 0x31a, 0x31f, 0x324, 0x329,
    0x32e, 0x333, 0x338, 0x33d, 0x342, 0x347, 0x34c, 0x351,
    0x356, 0x35b, 0x360, 0x365, 0x36a, 0x370, 0x375, 0x37a,
    0x37f, 0x384, 0x38a, 0x38f, 0x394, 0x399, 0x39f, 0x3a4,
    0x3a9, 0x3ae, 0x3b4, 0x3b9, 0x3bf, 0x3c4, 0x3c9, 0x3cf,
    0x3d4, 0x3da, 0x3df, 0x3e4, 0x3ea, 0x3ef, 0x3f5, 0x3fa,
];

//
// freq mult table multiplied by 2
//
static mt: [u8; 16] = [
    1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30,
];

//
// ksl table
//
static kslrom: [u8; 16] = [
    0, 32, 40, 45, 48, 51, 53, 55, 56, 58, 59, 60, 61, 62, 63, 64,
];

static kslshift: [u8; 4] = [8, 1, 2, 0];

//
// envelope generator constants
//
static eg_incstep: [[[u8; 8]; 4]; 3] = [
    [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
    ],
    [
        [0, 1, 0, 1, 0, 1, 0, 1],
        [0, 1, 0, 1, 1, 1, 0, 1],
        [0, 1, 1, 1, 0, 1, 1, 1],
        [0, 1, 1, 1, 1, 1, 1, 1],
    ],
    [
        [1, 1, 1, 1, 1, 1, 1, 1],
        [2, 2, 1, 1, 1, 1, 1, 1],
        [2, 2, 1, 1, 2, 2, 1, 1],
        [2, 2, 2, 2, 2, 2, 1, 1],
    ],
];

static eg_incdesc: [u8; 16] = [0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2];

static eg_incsh: [i8; 16] = [0, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, -1, -2];

//
// address decoding
//
static ad_slot: [i8; 0x20] = [
    0, 1, 2, 3, 4, 5, -1, -1, 6, 7, 8, 9, 10, 11, -1, -1,
    12, 13, 14, 15, 16, 17, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
];

static ch_slot: [u8; 18] = [
    0, 1, 2, 6, 7, 8, 12, 13, 14, 18, 19, 20, 24, 25, 26, 30, 31, 32,
];

//
// Envelope generator
//

static envelope_sin: [unsafe fn(u16, u16) -> i16; 8] = [
    OPL3_EnvelopeCalcSin0,
    OPL3_EnvelopeCalcSin1,
    OPL3_EnvelopeCalcSin2,
    OPL3_EnvelopeCalcSin3,
    OPL3_EnvelopeCalcSin4,
    OPL3_EnvelopeCalcSin5,
    OPL3_EnvelopeCalcSin6,
    OPL3_EnvelopeCalcSin7,
];

static envelope_gen: [unsafe fn(*mut opl3_slot); 5] = [
    OPL3_EnvelopeGenOff,
    OPL3_EnvelopeGenAttack,
    OPL3_EnvelopeGenDecay,
    OPL3_EnvelopeGenSustain,
    OPL3_EnvelopeGenRelease,
];

unsafe fn OPL3_EnvelopeCalcExp(mut level: u32) -> i16 {
    if level > 0x1fff {
        level = 0x1fff;
    }
    ((((exprom[((level & 0xff) ^ 0xff) as usize] as u32) | 0x400) << 1) >> (level >> 8)) as i16
}

unsafe fn OPL3_EnvelopeCalcSin0(mut phase: u16, envelope: u16) -> i16 {
    let mut out: u16 = 0;
    let mut neg: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x200 != 0 {
        neg = !0;
    }
    if phase & 0x100 != 0 {
        out = logsinrom[((phase & 0xff) ^ 0xff) as usize];
    } else {
        out = logsinrom[(phase & 0xff) as usize];
    }
    (OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3)) as u16 ^ neg) as i16
}

unsafe fn OPL3_EnvelopeCalcSin1(mut phase: u16, envelope: u16) -> i16 {
    let mut out: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x200 != 0 {
        out = 0x1000;
    } else if phase & 0x100 != 0 {
        out = logsinrom[((phase & 0xff) ^ 0xff) as usize];
    } else {
        out = logsinrom[(phase & 0xff) as usize];
    }
    OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3))
}

unsafe fn OPL3_EnvelopeCalcSin2(mut phase: u16, envelope: u16) -> i16 {
    let mut out: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x100 != 0 {
        out = logsinrom[((phase & 0xff) ^ 0xff) as usize];
    } else {
        out = logsinrom[(phase & 0xff) as usize];
    }
    OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3))
}

unsafe fn OPL3_EnvelopeCalcSin3(mut phase: u16, envelope: u16) -> i16 {
    let mut out: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x100 != 0 {
        out = 0x1000;
    } else {
        out = logsinrom[(phase & 0xff) as usize];
    }
    OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3))
}

unsafe fn OPL3_EnvelopeCalcSin4(mut phase: u16, envelope: u16) -> i16 {
    let mut out: u16 = 0;
    let mut neg: u16 = 0;
    phase &= 0x3ff;
    if (phase & 0x300) == 0x100 {
        neg = !0;
    }
    if phase & 0x200 != 0 {
        out = 0x1000;
    } else if phase & 0x80 != 0 {
        out = logsinrom[(((phase ^ 0xff) << 1) & 0xff) as usize];
    } else {
        out = logsinrom[((phase << 1) & 0xff) as usize];
    }
    (OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3)) as u16 ^ neg) as i16
}

unsafe fn OPL3_EnvelopeCalcSin5(mut phase: u16, envelope: u16) -> i16 {
    let mut out: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x200 != 0 {
        out = 0x1000;
    } else if phase & 0x80 != 0 {
        out = logsinrom[(((phase ^ 0xff) << 1) & 0xff) as usize];
    } else {
        out = logsinrom[((phase << 1) & 0xff) as usize];
    }
    OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3))
}

unsafe fn OPL3_EnvelopeCalcSin6(mut phase: u16, envelope: u16) -> i16 {
    let mut neg: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x200 != 0 {
        neg = !0;
    }
    (OPL3_EnvelopeCalcExp((envelope as u32) << 3) as u16 ^ neg) as i16
}

unsafe fn OPL3_EnvelopeCalcSin7(mut phase: u16, envelope: u16) -> i16 {
    let out: u16;
    let mut neg: u16 = 0;
    phase &= 0x3ff;
    if phase & 0x200 != 0 {
        neg = !0;
        phase = (phase & 0x1ff) ^ 0x1ff;
    }
    out = phase << 3;
    (OPL3_EnvelopeCalcExp((out as u32) + ((envelope as u32) << 3)) as u16 ^ neg) as i16
}

unsafe fn OPL3_EnvelopeCalcRate(slot: *mut opl3_slot, reg_rate: u8) -> u8 {
    if reg_rate == 0x00 {
        return 0x00;
    }
    let ksv = (*(*slot).channel).ksv;
    let mut rate: u8 = (((reg_rate as u32) << 2)
        + (if (*slot).reg_ksr != 0 {
            ksv as u32
        } else {
            (ksv as u32) >> 2
        })) as u8;
    if rate > 0x3c {
        rate = 0x3c;
    }
    rate
}

unsafe fn OPL3_EnvelopeUpdateKSL(slot: *mut opl3_slot) {
    let mut ksl: i32 = ((kslrom[((*(*slot).channel).f_num >> 6) as usize] as i32) << 2)
        - ((0x08 - (*(*slot).channel).block as i32) << 5);
    if ksl < 0 {
        ksl = 0;
    }
    (*slot).eg_ksl = ksl as u8;
}

unsafe fn OPL3_EnvelopeUpdateRate(slot: *mut opl3_slot) {
    match (*slot).eg_gen {
        envelope_gen_num_off | envelope_gen_num_attack => {
            (*slot).eg_rate = OPL3_EnvelopeCalcRate(slot, (*slot).reg_ar);
        }
        envelope_gen_num_decay => {
            (*slot).eg_rate = OPL3_EnvelopeCalcRate(slot, (*slot).reg_dr);
        }
        envelope_gen_num_sustain | envelope_gen_num_release => {
            (*slot).eg_rate = OPL3_EnvelopeCalcRate(slot, (*slot).reg_rr);
        }
        _ => {}
    }
}

unsafe fn OPL3_EnvelopeGenOff(slot: *mut opl3_slot) {
    (*slot).eg_rout = 0x1ff;
}

unsafe fn OPL3_EnvelopeGenAttack(slot: *mut opl3_slot) {
    if (*slot).eg_rout == 0x00 {
        (*slot).eg_gen = envelope_gen_num_decay;
        OPL3_EnvelopeUpdateRate(slot);
        return;
    }
    (*slot).eg_rout = ((*slot).eg_rout as i32
        + ((!((*slot).eg_rout as i32)) * ((*slot).eg_inc as i32) >> 3)) as i16;
    if (*slot).eg_rout < 0x00 {
        (*slot).eg_rout = 0x00;
    }
}

unsafe fn OPL3_EnvelopeGenDecay(slot: *mut opl3_slot) {
    if (*slot).eg_rout as i32 >= ((*slot).reg_sl as i32) << 4 {
        (*slot).eg_gen = envelope_gen_num_sustain;
        OPL3_EnvelopeUpdateRate(slot);
        return;
    }
    (*slot).eg_rout = (*slot).eg_rout.wrapping_add((*slot).eg_inc as i16);
}

unsafe fn OPL3_EnvelopeGenSustain(slot: *mut opl3_slot) {
    if (*slot).reg_type == 0 {
        OPL3_EnvelopeGenRelease(slot);
    }
}

unsafe fn OPL3_EnvelopeGenRelease(slot: *mut opl3_slot) {
    if (*slot).eg_rout >= 0x1ff {
        (*slot).eg_gen = envelope_gen_num_off;
        (*slot).eg_rout = 0x1ff;
        OPL3_EnvelopeUpdateRate(slot);
        return;
    }
    (*slot).eg_rout = (*slot).eg_rout.wrapping_add((*slot).eg_inc as i16);
}

unsafe fn OPL3_EnvelopeCalc(slot: *mut opl3_slot) {
    let rate_h: u8;
    let rate_l: u8;
    let mut inc: u8 = 0;
    rate_h = (*slot).eg_rate >> 2;
    rate_l = (*slot).eg_rate & 3;
    let chip_timer: u32 = (*(*slot).chip).timer as u32;
    if eg_incsh[rate_h as usize] > 0 {
        if (chip_timer & ((1u32 << eg_incsh[rate_h as usize] as u32) - 1)) == 0 {
            inc = eg_incstep[eg_incdesc[rate_h as usize] as usize][rate_l as usize]
                [((chip_timer >> eg_incsh[rate_h as usize] as u32) & 0x07) as usize];
        }
    } else {
        inc = ((eg_incstep[eg_incdesc[rate_h as usize] as usize][rate_l as usize]
            [(chip_timer & 0x07) as usize] as u32)
            << ((-(eg_incsh[rate_h as usize] as i32)) as u32)) as u8;
    }
    (*slot).eg_inc = inc;
    (*slot).eg_out = ((*slot).eg_rout as i32
        + (((*slot).reg_tl as i32) << 2)
        + (((*slot).eg_ksl as i32) >> kslshift[(*slot).reg_ksl as usize] as i32)
        + (*(*slot).trem) as i32) as i16;
    envelope_gen[(*slot).eg_gen as usize](slot);
}

unsafe fn OPL3_EnvelopeKeyOn(slot: *mut opl3_slot, type_: u8) {
    if (*slot).key == 0 {
        (*slot).eg_gen = envelope_gen_num_attack;
        OPL3_EnvelopeUpdateRate(slot);
        if ((*slot).eg_rate >> 2) == 0x0f {
            (*slot).eg_gen = envelope_gen_num_decay;
            OPL3_EnvelopeUpdateRate(slot);
            (*slot).eg_rout = 0x00;
        }
        (*slot).pg_phase = 0x00;
    }
    (*slot).key |= type_;
}

unsafe fn OPL3_EnvelopeKeyOff(slot: *mut opl3_slot, type_: u8) {
    if (*slot).key != 0 {
        (*slot).key &= !type_;
        if (*slot).key == 0 {
            (*slot).eg_gen = envelope_gen_num_release;
            OPL3_EnvelopeUpdateRate(slot);
        }
    }
}

//
// Phase Generator
//

unsafe fn OPL3_PhaseGenerate(slot: *mut opl3_slot) {
    let mut f_num: u16;
    let basefreq: u32;

    f_num = (*(*slot).channel).f_num;
    if (*slot).reg_vib != 0 {
        let mut range: i8;
        let vibpos: u8;

        range = ((f_num >> 7) & 7) as i8;
        vibpos = (*(*slot).chip).vibpos;

        if (vibpos & 3) == 0 {
            range = 0;
        } else if vibpos & 1 != 0 {
            range >>= 1;
        }
        range >>= (*(*slot).chip).vibshift;

        if vibpos & 4 != 0 {
            range = -range;
        }
        f_num = f_num.wrapping_add((range as i16) as u16);
    }
    basefreq = ((f_num as u32) << (*(*slot).channel).block) >> 1;
    (*slot).pg_phase = (*slot).pg_phase.wrapping_add(
        basefreq.wrapping_mul(mt[(*slot).reg_mult as usize] as u32) >> 1,
    );
}

//
// Noise Generator
//

unsafe fn OPL3_NoiseGenerate(chip: *mut opl3_chip) {
    if (*chip).noise & 0x01 != 0 {
        (*chip).noise ^= 0x800302;
    }
    (*chip).noise >>= 1;
}

//
// Slot
//

unsafe fn OPL3_SlotWrite20(slot: *mut opl3_slot, data: u8) {
    if (data >> 7) & 0x01 != 0 {
        (*slot).trem = &mut (*(*slot).chip).tremolo as *mut u8;
    } else {
        (*slot).trem = &mut (*(*slot).chip).zeromod as *mut i16 as *mut u8;
    }
    (*slot).reg_vib = (data >> 6) & 0x01;
    (*slot).reg_type = (data >> 5) & 0x01;
    (*slot).reg_ksr = (data >> 4) & 0x01;
    (*slot).reg_mult = data & 0x0f;
    OPL3_EnvelopeUpdateRate(slot);
}

unsafe fn OPL3_SlotWrite40(slot: *mut opl3_slot, data: u8) {
    (*slot).reg_ksl = (data >> 6) & 0x03;
    (*slot).reg_tl = data & 0x3f;
    OPL3_EnvelopeUpdateKSL(slot);
}

unsafe fn OPL3_SlotWrite60(slot: *mut opl3_slot, data: u8) {
    (*slot).reg_ar = (data >> 4) & 0x0f;
    (*slot).reg_dr = data & 0x0f;
    OPL3_EnvelopeUpdateRate(slot);
}

unsafe fn OPL3_SlotWrite80(slot: *mut opl3_slot, data: u8) {
    (*slot).reg_sl = (data >> 4) & 0x0f;
    if (*slot).reg_sl == 0x0f {
        (*slot).reg_sl = 0x1f;
    }
    (*slot).reg_rr = data & 0x0f;
    OPL3_EnvelopeUpdateRate(slot);
}

unsafe fn OPL3_SlotWriteE0(slot: *mut opl3_slot, data: u8) {
    (*slot).reg_wf = data & 0x07;
    if (*(*slot).chip).newm == 0x00 {
        (*slot).reg_wf &= 0x03;
    }
}

unsafe fn OPL3_SlotGeneratePhase(slot: *mut opl3_slot, phase: u16) {
    (*slot).out = envelope_sin[(*slot).reg_wf as usize](phase, (*slot).eg_out as u16);
}

unsafe fn OPL3_SlotGenerate(slot: *mut opl3_slot) {
    OPL3_SlotGeneratePhase(
        slot,
        (((*slot).pg_phase >> 9) as u16).wrapping_add((*(*slot).mod_) as u16),
    );
}

unsafe fn OPL3_SlotGenerateZM(slot: *mut opl3_slot) {
    OPL3_SlotGeneratePhase(slot, ((*slot).pg_phase >> 9) as u16);
}

unsafe fn OPL3_SlotCalcFB(slot: *mut opl3_slot) {
    if (*(*slot).channel).fb != 0x00 {
        (*slot).fbmod = (((*slot).prout as i32 + (*slot).out as i32)
            >> (0x09 - (*(*slot).channel).fb as i32)) as i16;
    } else {
        (*slot).fbmod = 0;
    }
    (*slot).prout = (*slot).out;
}

//
// Channel
//

unsafe fn OPL3_ChannelUpdateRhythm(chip: *mut opl3_chip, data: u8) {
    let channel6: *mut opl3_channel;
    let channel7: *mut opl3_channel;
    let channel8: *mut opl3_channel;
    let mut chnum: u8;

    (*chip).rhy = data & 0x3f;
    if (*chip).rhy & 0x20 != 0 {
        channel6 = &mut (*chip).channel[6] as *mut opl3_channel;
        channel7 = &mut (*chip).channel[7] as *mut opl3_channel;
        channel8 = &mut (*chip).channel[8] as *mut opl3_channel;
        (*channel6).out[0] = &mut (*(*channel6).slots[1]).out as *mut i16;
        (*channel6).out[1] = &mut (*(*channel6).slots[1]).out as *mut i16;
        (*channel6).out[2] = &mut (*chip).zeromod as *mut i16;
        (*channel6).out[3] = &mut (*chip).zeromod as *mut i16;
        (*channel7).out[0] = &mut (*(*channel7).slots[0]).out as *mut i16;
        (*channel7).out[1] = &mut (*(*channel7).slots[0]).out as *mut i16;
        (*channel7).out[2] = &mut (*(*channel7).slots[1]).out as *mut i16;
        (*channel7).out[3] = &mut (*(*channel7).slots[1]).out as *mut i16;
        (*channel8).out[0] = &mut (*(*channel8).slots[0]).out as *mut i16;
        (*channel8).out[1] = &mut (*(*channel8).slots[0]).out as *mut i16;
        (*channel8).out[2] = &mut (*(*channel8).slots[1]).out as *mut i16;
        (*channel8).out[3] = &mut (*(*channel8).slots[1]).out as *mut i16;
        chnum = 6;
        while chnum < 9 {
            (*chip).channel[chnum as usize].chtype = ch_drum;
            chnum += 1;
        }
        OPL3_ChannelSetupAlg(channel6);
        // hh
        if (*chip).rhy & 0x01 != 0 {
            OPL3_EnvelopeKeyOn((*channel7).slots[0], egk_drum);
        } else {
            OPL3_EnvelopeKeyOff((*channel7).slots[0], egk_drum);
        }
        // tc
        if (*chip).rhy & 0x02 != 0 {
            OPL3_EnvelopeKeyOn((*channel8).slots[1], egk_drum);
        } else {
            OPL3_EnvelopeKeyOff((*channel8).slots[1], egk_drum);
        }
        // tom
        if (*chip).rhy & 0x04 != 0 {
            OPL3_EnvelopeKeyOn((*channel8).slots[0], egk_drum);
        } else {
            OPL3_EnvelopeKeyOff((*channel8).slots[0], egk_drum);
        }
        // sd
        if (*chip).rhy & 0x08 != 0 {
            OPL3_EnvelopeKeyOn((*channel7).slots[1], egk_drum);
        } else {
            OPL3_EnvelopeKeyOff((*channel7).slots[1], egk_drum);
        }
        // bd
        if (*chip).rhy & 0x10 != 0 {
            OPL3_EnvelopeKeyOn((*channel6).slots[0], egk_drum);
            OPL3_EnvelopeKeyOn((*channel6).slots[1], egk_drum);
        } else {
            OPL3_EnvelopeKeyOff((*channel6).slots[0], egk_drum);
            OPL3_EnvelopeKeyOff((*channel6).slots[1], egk_drum);
        }
    } else {
        chnum = 6;
        while chnum < 9 {
            (*chip).channel[chnum as usize].chtype = ch_2op;
            OPL3_ChannelSetupAlg(&mut (*chip).channel[chnum as usize] as *mut opl3_channel);
            OPL3_EnvelopeKeyOff((*chip).channel[chnum as usize].slots[0], egk_drum);
            OPL3_EnvelopeKeyOff((*chip).channel[chnum as usize].slots[1], egk_drum);
            chnum += 1;
        }
    }
}

unsafe fn OPL3_ChannelWriteA0(channel: *mut opl3_channel, data: u8) {
    if (*(*channel).chip).newm != 0 && (*channel).chtype == ch_4op2 {
        return;
    }
    (*channel).f_num = ((*channel).f_num & 0x300) | data as u16;
    (*channel).ksv = (((*channel).block << 1)
        | (((*channel).f_num >> (0x09 - (*(*channel).chip).nts)) & 0x01) as u8) as u8;
    OPL3_EnvelopeUpdateKSL((*channel).slots[0]);
    OPL3_EnvelopeUpdateKSL((*channel).slots[1]);
    OPL3_EnvelopeUpdateRate((*channel).slots[0]);
    OPL3_EnvelopeUpdateRate((*channel).slots[1]);
    if (*(*channel).chip).newm != 0 && (*channel).chtype == ch_4op {
        (*(*channel).pair).f_num = (*channel).f_num;
        (*(*channel).pair).ksv = (*channel).ksv;
        OPL3_EnvelopeUpdateKSL((*(*channel).pair).slots[0]);
        OPL3_EnvelopeUpdateKSL((*(*channel).pair).slots[1]);
        OPL3_EnvelopeUpdateRate((*(*channel).pair).slots[0]);
        OPL3_EnvelopeUpdateRate((*(*channel).pair).slots[1]);
    }
}

unsafe fn OPL3_ChannelWriteB0(channel: *mut opl3_channel, data: u8) {
    if (*(*channel).chip).newm != 0 && (*channel).chtype == ch_4op2 {
        return;
    }
    (*channel).f_num = ((*channel).f_num & 0xff) | (((data as u16) & 0x03) << 8);
    (*channel).block = (data >> 2) & 0x07;
    (*channel).ksv = (((*channel).block << 1)
        | (((*channel).f_num >> (0x09 - (*(*channel).chip).nts)) & 0x01) as u8) as u8;
    OPL3_EnvelopeUpdateKSL((*channel).slots[0]);
    OPL3_EnvelopeUpdateKSL((*channel).slots[1]);
    OPL3_EnvelopeUpdateRate((*channel).slots[0]);
    OPL3_EnvelopeUpdateRate((*channel).slots[1]);
    if (*(*channel).chip).newm != 0 && (*channel).chtype == ch_4op {
        (*(*channel).pair).f_num = (*channel).f_num;
        (*(*channel).pair).block = (*channel).block;
        (*(*channel).pair).ksv = (*channel).ksv;
        OPL3_EnvelopeUpdateKSL((*(*channel).pair).slots[0]);
        OPL3_EnvelopeUpdateKSL((*(*channel).pair).slots[1]);
        OPL3_EnvelopeUpdateRate((*(*channel).pair).slots[0]);
        OPL3_EnvelopeUpdateRate((*(*channel).pair).slots[1]);
    }
}

unsafe fn OPL3_ChannelSetupAlg(channel: *mut opl3_channel) {
    if (*channel).chtype == ch_drum {
        match (*channel).alg & 0x01 {
            0x00 => {
                (*(*channel).slots[0]).mod_ = &mut (*(*channel).slots[0]).fbmod as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).slots[0]).out as *mut i16;
            }
            0x01 => {
                (*(*channel).slots[0]).mod_ = &mut (*(*channel).slots[0]).fbmod as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            _ => {}
        }
        return;
    }
    if (*channel).alg & 0x08 != 0 {
        return;
    }
    if (*channel).alg & 0x04 != 0 {
        (*(*channel).pair).out[0] = &mut (*(*channel).chip).zeromod as *mut i16;
        (*(*channel).pair).out[1] = &mut (*(*channel).chip).zeromod as *mut i16;
        (*(*channel).pair).out[2] = &mut (*(*channel).chip).zeromod as *mut i16;
        (*(*channel).pair).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
        match (*channel).alg & 0x03 {
            0x00 => {
                (*(*(*channel).pair).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[0]).fbmod as *mut i16;
                (*(*(*channel).pair).slots[1]).mod_ =
                    &mut (*(*(*channel).pair).slots[0]).out as *mut i16;
                (*(*channel).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[1]).out as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).slots[0]).out as *mut i16;
                (*channel).out[0] = &mut (*(*channel).slots[1]).out as *mut i16;
                (*channel).out[1] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[2] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            0x01 => {
                (*(*(*channel).pair).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[0]).fbmod as *mut i16;
                (*(*(*channel).pair).slots[1]).mod_ =
                    &mut (*(*(*channel).pair).slots[0]).out as *mut i16;
                (*(*channel).slots[0]).mod_ = &mut (*(*channel).chip).zeromod as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).slots[0]).out as *mut i16;
                (*channel).out[0] = &mut (*(*(*channel).pair).slots[1]).out as *mut i16;
                (*channel).out[1] = &mut (*(*channel).slots[1]).out as *mut i16;
                (*channel).out[2] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            0x02 => {
                (*(*(*channel).pair).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[0]).fbmod as *mut i16;
                (*(*(*channel).pair).slots[1]).mod_ = &mut (*(*channel).chip).zeromod as *mut i16;
                (*(*channel).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[1]).out as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).slots[0]).out as *mut i16;
                (*channel).out[0] = &mut (*(*(*channel).pair).slots[0]).out as *mut i16;
                (*channel).out[1] = &mut (*(*channel).slots[1]).out as *mut i16;
                (*channel).out[2] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            0x03 => {
                (*(*(*channel).pair).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[0]).fbmod as *mut i16;
                (*(*(*channel).pair).slots[1]).mod_ = &mut (*(*channel).chip).zeromod as *mut i16;
                (*(*channel).slots[0]).mod_ =
                    &mut (*(*(*channel).pair).slots[1]).out as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[0] = &mut (*(*(*channel).pair).slots[0]).out as *mut i16;
                (*channel).out[1] = &mut (*(*channel).slots[0]).out as *mut i16;
                (*channel).out[2] = &mut (*(*channel).slots[1]).out as *mut i16;
                (*channel).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            _ => {}
        }
    } else {
        match (*channel).alg & 0x01 {
            0x00 => {
                (*(*channel).slots[0]).mod_ = &mut (*(*channel).slots[0]).fbmod as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).slots[0]).out as *mut i16;
                (*channel).out[0] = &mut (*(*channel).slots[1]).out as *mut i16;
                (*channel).out[1] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[2] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            0x01 => {
                (*(*channel).slots[0]).mod_ = &mut (*(*channel).slots[0]).fbmod as *mut i16;
                (*(*channel).slots[1]).mod_ = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[0] = &mut (*(*channel).slots[0]).out as *mut i16;
                (*channel).out[1] = &mut (*(*channel).slots[1]).out as *mut i16;
                (*channel).out[2] = &mut (*(*channel).chip).zeromod as *mut i16;
                (*channel).out[3] = &mut (*(*channel).chip).zeromod as *mut i16;
            }
            _ => {}
        }
    }
}

unsafe fn OPL3_ChannelWriteC0(channel: *mut opl3_channel, data: u8) {
    (*channel).fb = (data & 0x0e) >> 1;
    (*channel).con = data & 0x01;
    (*channel).alg = (*channel).con;
    if (*(*channel).chip).newm != 0 {
        if (*channel).chtype == ch_4op {
            (*(*channel).pair).alg = 0x04 | ((*channel).con << 1) | (*(*channel).pair).con;
            (*channel).alg = 0x08;
            OPL3_ChannelSetupAlg((*channel).pair);
        } else if (*channel).chtype == ch_4op2 {
            (*channel).alg = 0x04 | ((*(*channel).pair).con << 1) | (*channel).con;
            (*(*channel).pair).alg = 0x08;
            OPL3_ChannelSetupAlg(channel);
        } else {
            OPL3_ChannelSetupAlg(channel);
        }
    } else {
        OPL3_ChannelSetupAlg(channel);
    }
    if (*(*channel).chip).newm != 0 {
        (*channel).cha = if (data >> 4) & 0x01 != 0 { !0 } else { 0 };
        (*channel).chb = if (data >> 5) & 0x01 != 0 { !0 } else { 0 };
    } else {
        (*channel).cha = !0;
        (*channel).chb = !0;
    }
}

unsafe fn OPL3_ChannelKeyOn(channel: *mut opl3_channel) {
    if (*(*channel).chip).newm != 0 {
        if (*channel).chtype == ch_4op {
            OPL3_EnvelopeKeyOn((*channel).slots[0], egk_norm);
            OPL3_EnvelopeKeyOn((*channel).slots[1], egk_norm);
            OPL3_EnvelopeKeyOn((*(*channel).pair).slots[0], egk_norm);
            OPL3_EnvelopeKeyOn((*(*channel).pair).slots[1], egk_norm);
        } else if (*channel).chtype == ch_2op || (*channel).chtype == ch_drum {
            OPL3_EnvelopeKeyOn((*channel).slots[0], egk_norm);
            OPL3_EnvelopeKeyOn((*channel).slots[1], egk_norm);
        }
    } else {
        OPL3_EnvelopeKeyOn((*channel).slots[0], egk_norm);
        OPL3_EnvelopeKeyOn((*channel).slots[1], egk_norm);
    }
}

unsafe fn OPL3_ChannelKeyOff(channel: *mut opl3_channel) {
    if (*(*channel).chip).newm != 0 {
        if (*channel).chtype == ch_4op {
            OPL3_EnvelopeKeyOff((*channel).slots[0], egk_norm);
            OPL3_EnvelopeKeyOff((*channel).slots[1], egk_norm);
            OPL3_EnvelopeKeyOff((*(*channel).pair).slots[0], egk_norm);
            OPL3_EnvelopeKeyOff((*(*channel).pair).slots[1], egk_norm);
        } else if (*channel).chtype == ch_2op || (*channel).chtype == ch_drum {
            OPL3_EnvelopeKeyOff((*channel).slots[0], egk_norm);
            OPL3_EnvelopeKeyOff((*channel).slots[1], egk_norm);
        }
    } else {
        OPL3_EnvelopeKeyOff((*channel).slots[0], egk_norm);
        OPL3_EnvelopeKeyOff((*channel).slots[1], egk_norm);
    }
}

unsafe fn OPL3_ChannelSet4Op(chip: *mut opl3_chip, data: u8) {
    let mut bit: u8;
    let mut chnum: u8;
    bit = 0;
    while bit < 6 {
        chnum = bit;
        if bit >= 3 {
            chnum += 9 - 3;
        }
        if (data >> bit) & 0x01 != 0 {
            (*chip).channel[chnum as usize].chtype = ch_4op;
            (*chip).channel[(chnum + 3) as usize].chtype = ch_4op2;
        } else {
            (*chip).channel[chnum as usize].chtype = ch_2op;
            (*chip).channel[(chnum + 3) as usize].chtype = ch_2op;
        }
        bit += 1;
    }
}

unsafe fn OPL3_ClipSample(mut sample: i32) -> i16 {
    if sample > 32767 {
        sample = 32767;
    } else if sample < -32768 {
        sample = -32768;
    }
    sample as i16
}

unsafe fn OPL3_GenerateRhythm1(chip: *mut opl3_chip) {
    let channel6: *mut opl3_channel;
    let channel7: *mut opl3_channel;
    let channel8: *mut opl3_channel;
    let phase14: u16;
    let phase17: u16;
    let mut phase: u16;
    let phasebit: u16;

    channel6 = &mut (*chip).channel[6] as *mut opl3_channel;
    channel7 = &mut (*chip).channel[7] as *mut opl3_channel;
    channel8 = &mut (*chip).channel[8] as *mut opl3_channel;
    OPL3_SlotGenerate((*channel6).slots[0]);
    phase14 = (((*(*channel7).slots[0]).pg_phase >> 9) & 0x3ff) as u16;
    phase17 = (((*(*channel8).slots[1]).pg_phase >> 9) & 0x3ff) as u16;
    phase = 0x00;
    // hh tc phase bit
    phasebit = if (phase14 & 0x08)
        | (((phase14 >> 5) ^ phase14) & 0x04)
        | (((phase17 >> 2) ^ phase17) & 0x08)
        != 0
    {
        0x01
    } else {
        0x00
    };
    // hh
    phase = (phasebit << 9)
        | (0x34u16 << ((phasebit ^ ((*chip).noise & 0x01) as u16) << 1));
    OPL3_SlotGeneratePhase((*channel7).slots[0], phase);
    // tt
    OPL3_SlotGenerateZM((*channel8).slots[0]);
}

unsafe fn OPL3_GenerateRhythm2(chip: *mut opl3_chip) {
    let channel6: *mut opl3_channel;
    let channel7: *mut opl3_channel;
    let channel8: *mut opl3_channel;
    let phase14: u16;
    let phase17: u16;
    let mut phase: u16;
    let phasebit: u16;

    channel6 = &mut (*chip).channel[6] as *mut opl3_channel;
    channel7 = &mut (*chip).channel[7] as *mut opl3_channel;
    channel8 = &mut (*chip).channel[8] as *mut opl3_channel;
    OPL3_SlotGenerate((*channel6).slots[1]);
    phase14 = (((*(*channel7).slots[0]).pg_phase >> 9) & 0x3ff) as u16;
    phase17 = (((*(*channel8).slots[1]).pg_phase >> 9) & 0x3ff) as u16;
    phase = 0x00;
    // hh tc phase bit
    phasebit = if (phase14 & 0x08)
        | (((phase14 >> 5) ^ phase14) & 0x04)
        | (((phase17 >> 2) ^ phase17) & 0x08)
        != 0
    {
        0x01
    } else {
        0x00
    };
    // sd
    phase = (0x100u16 << ((phase14 >> 8) & 0x01)) ^ (((*chip).noise & 0x01) as u16) << 8;
    OPL3_SlotGeneratePhase((*channel7).slots[1], phase);
    // tc
    phase = 0x100 | (phasebit << 9);
    OPL3_SlotGeneratePhase((*channel8).slots[1], phase);
}

#[no_mangle]
pub unsafe extern "C" fn OPL3_Generate(chip: *mut opl3_chip, buf: *mut i16) {
    let mut mixed: i32;

    *buf.add(1) = OPL3_ClipSample((*chip).mixbuff[1]);

    for ii in 0..12usize {
        OPL3_SlotCalcFB(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_PhaseGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_EnvelopeCalc(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
    }

    for ii in 12..15usize {
        OPL3_SlotCalcFB(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_PhaseGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_EnvelopeCalc(&mut (*chip).slot[ii] as *mut opl3_slot);
    }

    if (*chip).rhy & 0x20 != 0 {
        OPL3_GenerateRhythm1(chip);
    } else {
        OPL3_SlotGenerate(&mut (*chip).slot[12] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[13] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[14] as *mut opl3_slot);
    }

    mixed = 0;
    for ii in 0..18usize {
        let chan = &(*chip).channel[ii] as *const opl3_channel;
        let chanout = (*chan).out;
        let accm = ((*chanout[0]) as i32
            + (*chanout[1]) as i32
            + (*chanout[2]) as i32
            + (*chanout[3]) as i32) as i16;
        mixed = mixed.wrapping_add(((accm as u16 & (*chan).cha) as i16) as i32);
    }
    (*chip).mixbuff[0] = mixed;

    for ii in 15..18usize {
        OPL3_SlotCalcFB(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_PhaseGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_EnvelopeCalc(&mut (*chip).slot[ii] as *mut opl3_slot);
    }

    if (*chip).rhy & 0x20 != 0 {
        OPL3_GenerateRhythm2(chip);
    } else {
        OPL3_SlotGenerate(&mut (*chip).slot[15] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[16] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[17] as *mut opl3_slot);
    }

    *buf.add(0) = OPL3_ClipSample((*chip).mixbuff[0]);

    for ii in 18..33usize {
        OPL3_SlotCalcFB(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_PhaseGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_EnvelopeCalc(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
    }

    mixed = 0;
    for ii in 0..18usize {
        let chan = &(*chip).channel[ii] as *const opl3_channel;
        let chanout = (*chan).out;
        let accm = ((*chanout[0]) as i32
            + (*chanout[1]) as i32
            + (*chanout[2]) as i32
            + (*chanout[3]) as i32) as i16;
        mixed = mixed.wrapping_add(((accm as u16 & (*chan).chb) as i16) as i32);
    }

    (*chip).mixbuff[1] = mixed;

    for ii in 33..36usize {
        OPL3_SlotCalcFB(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_PhaseGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_EnvelopeCalc(&mut (*chip).slot[ii] as *mut opl3_slot);
        OPL3_SlotGenerate(&mut (*chip).slot[ii] as *mut opl3_slot);
    }

    OPL3_NoiseGenerate(chip);

    if ((*chip).timer & 0x3f) == 0x3f {
        (*chip).tremolopos = ((*chip).tremolopos + 1) % 210;
    }
    if (*chip).tremolopos < 105 {
        (*chip).tremolo = (*chip).tremolopos >> (*chip).tremoloshift;
    } else {
        (*chip).tremolo = (210 - (*chip).tremolopos) >> (*chip).tremoloshift;
    }

    if ((*chip).timer & 0x3ff) == 0x3ff {
        (*chip).vibpos = ((*chip).vibpos + 1) & 7;
    }

    (*chip).timer = (*chip).timer.wrapping_add(1);

    while (*chip).writebuf[(*chip).writebuf_cur as usize].time <= (*chip).writebuf_samplecnt {
        if (*chip).writebuf[(*chip).writebuf_cur as usize].reg & 0x200 == 0 {
            break;
        }
        (*chip).writebuf[(*chip).writebuf_cur as usize].reg &= 0x1ff;
        OPL3_WriteReg(
            chip,
            (*chip).writebuf[(*chip).writebuf_cur as usize].reg,
            (*chip).writebuf[(*chip).writebuf_cur as usize].data,
        );
        (*chip).writebuf_cur = ((*chip).writebuf_cur + 1) % OPL_WRITEBUF_SIZE as u32;
    }
    (*chip).writebuf_samplecnt += 1;
}

#[no_mangle]
pub unsafe extern "C" fn OPL3_GenerateResampled(chip: *mut opl3_chip, buf: *mut i16) {
    while (*chip).samplecnt >= (*chip).rateratio {
        (*chip).oldsamples[0] = (*chip).samples[0];
        (*chip).oldsamples[1] = (*chip).samples[1];
        OPL3_Generate(chip, (*chip).samples.as_mut_ptr());
        (*chip).samplecnt -= (*chip).rateratio;
    }
    *buf.add(0) = (((*chip).oldsamples[0] as i32)
        .wrapping_mul((*chip).rateratio - (*chip).samplecnt)
        .wrapping_add(((*chip).samples[0] as i32).wrapping_mul((*chip).samplecnt))
        / (*chip).rateratio) as i16;
    *buf.add(1) = (((*chip).oldsamples[1] as i32)
        .wrapping_mul((*chip).rateratio - (*chip).samplecnt)
        .wrapping_add(((*chip).samples[1] as i32).wrapping_mul((*chip).samplecnt))
        / (*chip).rateratio) as i16;
    (*chip).samplecnt += 1 << RSM_FRAC;
}

#[no_mangle]
pub unsafe extern "C" fn OPL3_Reset(chip: *mut opl3_chip, samplerate: u32) {
    let mut slotnum: u8;
    let mut channum: u8;

    std::ptr::write_bytes(chip as *mut u8, 0, std::mem::size_of::<opl3_chip>());
    slotnum = 0;
    while slotnum < 36 {
        (*chip).slot[slotnum as usize].chip = chip;
        (*chip).slot[slotnum as usize].mod_ = &mut (*chip).zeromod as *mut i16;
        (*chip).slot[slotnum as usize].eg_rout = 0x1ff;
        (*chip).slot[slotnum as usize].eg_out = 0x1ff;
        (*chip).slot[slotnum as usize].eg_gen = envelope_gen_num_off;
        (*chip).slot[slotnum as usize].trem = &mut (*chip).zeromod as *mut i16 as *mut u8;
        slotnum += 1;
    }
    channum = 0;
    while channum < 18 {
        (*chip).channel[channum as usize].slots[0] =
            &mut (*chip).slot[ch_slot[channum as usize] as usize] as *mut opl3_slot;
        (*chip).channel[channum as usize].slots[1] =
            &mut (*chip).slot[(ch_slot[channum as usize] + 3) as usize] as *mut opl3_slot;
        (*chip).slot[ch_slot[channum as usize] as usize].channel =
            &mut (*chip).channel[channum as usize] as *mut opl3_channel;
        (*chip).slot[(ch_slot[channum as usize] + 3) as usize].channel =
            &mut (*chip).channel[channum as usize] as *mut opl3_channel;
        if (channum % 9) < 3 {
            (*chip).channel[channum as usize].pair =
                &mut (*chip).channel[(channum + 3) as usize] as *mut opl3_channel;
        } else if (channum % 9) < 6 {
            (*chip).channel[channum as usize].pair =
                &mut (*chip).channel[(channum - 3) as usize] as *mut opl3_channel;
        }
        (*chip).channel[channum as usize].chip = chip;
        (*chip).channel[channum as usize].out[0] = &mut (*chip).zeromod as *mut i16;
        (*chip).channel[channum as usize].out[1] = &mut (*chip).zeromod as *mut i16;
        (*chip).channel[channum as usize].out[2] = &mut (*chip).zeromod as *mut i16;
        (*chip).channel[channum as usize].out[3] = &mut (*chip).zeromod as *mut i16;
        (*chip).channel[channum as usize].chtype = ch_2op;
        (*chip).channel[channum as usize].cha = !0;
        (*chip).channel[channum as usize].chb = !0;
        OPL3_ChannelSetupAlg(&mut (*chip).channel[channum as usize] as *mut opl3_channel);
        channum += 1;
    }
    (*chip).noise = 0x306600;
    (*chip).rateratio = ((samplerate << RSM_FRAC) / 49716) as i32;
    (*chip).tremoloshift = 4;
    (*chip).vibshift = 1;
}

#[no_mangle]
pub unsafe extern "C" fn OPL3_WriteReg(chip: *mut opl3_chip, reg: u16, v: u8) {
    let high: u8 = ((reg >> 8) & 0x01) as u8;
    let regm: u8 = (reg & 0xff) as u8;
    match regm & 0xf0 {
        0x00 => {
            if high != 0 {
                match regm & 0x0f {
                    0x04 => {
                        OPL3_ChannelSet4Op(chip, v);
                    }
                    0x05 => {
                        (*chip).newm = v & 0x01;
                    }
                    _ => {}
                }
            } else {
                match regm & 0x0f {
                    0x08 => {
                        (*chip).nts = (v >> 6) & 0x01;
                    }
                    _ => {}
                }
            }
        }
        0x20 | 0x30 => {
            if ad_slot[(regm & 0x1f) as usize] >= 0 {
                OPL3_SlotWrite20(
                    &mut (*chip).slot
                        [18 * high as usize + ad_slot[(regm & 0x1f) as usize] as usize]
                        as *mut opl3_slot,
                    v,
                );
            }
        }
        0x40 | 0x50 => {
            if ad_slot[(regm & 0x1f) as usize] >= 0 {
                OPL3_SlotWrite40(
                    &mut (*chip).slot
                        [18 * high as usize + ad_slot[(regm & 0x1f) as usize] as usize]
                        as *mut opl3_slot,
                    v,
                );
            }
        }
        0x60 | 0x70 => {
            if ad_slot[(regm & 0x1f) as usize] >= 0 {
                OPL3_SlotWrite60(
                    &mut (*chip).slot
                        [18 * high as usize + ad_slot[(regm & 0x1f) as usize] as usize]
                        as *mut opl3_slot,
                    v,
                );
            }
        }
        0x80 | 0x90 => {
            if ad_slot[(regm & 0x1f) as usize] >= 0 {
                OPL3_SlotWrite80(
                    &mut (*chip).slot
                        [18 * high as usize + ad_slot[(regm & 0x1f) as usize] as usize]
                        as *mut opl3_slot,
                    v,
                );
            }
        }
        0xe0 | 0xf0 => {
            if ad_slot[(regm & 0x1f) as usize] >= 0 {
                OPL3_SlotWriteE0(
                    &mut (*chip).slot
                        [18 * high as usize + ad_slot[(regm & 0x1f) as usize] as usize]
                        as *mut opl3_slot,
                    v,
                );
            }
        }
        0xa0 => {
            if (regm & 0x0f) < 9 {
                OPL3_ChannelWriteA0(
                    &mut (*chip).channel[9 * high as usize + (regm & 0x0f) as usize]
                        as *mut opl3_channel,
                    v,
                );
            }
        }
        0xb0 => {
            if regm == 0xbd && high == 0 {
                (*chip).tremoloshift = (((v >> 7) ^ 1) << 1) + 2;
                (*chip).vibshift = ((v >> 6) & 0x01) ^ 1;
                OPL3_ChannelUpdateRhythm(chip, v);
            } else if (regm & 0x0f) < 9 {
                OPL3_ChannelWriteB0(
                    &mut (*chip).channel[9 * high as usize + (regm & 0x0f) as usize]
                        as *mut opl3_channel,
                    v,
                );
                if v & 0x20 != 0 {
                    OPL3_ChannelKeyOn(
                        &mut (*chip).channel[9 * high as usize + (regm & 0x0f) as usize]
                            as *mut opl3_channel,
                    );
                } else {
                    OPL3_ChannelKeyOff(
                        &mut (*chip).channel[9 * high as usize + (regm & 0x0f) as usize]
                            as *mut opl3_channel,
                    );
                }
            }
        }
        0xc0 => {
            if (regm & 0x0f) < 9 {
                OPL3_ChannelWriteC0(
                    &mut (*chip).channel[9 * high as usize + (regm & 0x0f) as usize]
                        as *mut opl3_channel,
                    v,
                );
            }
        }
        _ => {}
    }
}

#[no_mangle]
pub unsafe extern "C" fn OPL3_WriteRegBuffered(chip: *mut opl3_chip, reg: u16, v: u8) {
    let mut time1: u64;
    let time2: u64;

    if (*chip).writebuf[(*chip).writebuf_last as usize].reg & 0x200 != 0 {
        OPL3_WriteReg(
            chip,
            (*chip).writebuf[(*chip).writebuf_last as usize].reg & 0x1ff,
            (*chip).writebuf[(*chip).writebuf_last as usize].data,
        );

        (*chip).writebuf_cur = ((*chip).writebuf_last + 1) % OPL_WRITEBUF_SIZE as u32;
        (*chip).writebuf_samplecnt = (*chip).writebuf[(*chip).writebuf_last as usize].time;
    }

    (*chip).writebuf[(*chip).writebuf_last as usize].reg = reg | 0x200;
    (*chip).writebuf[(*chip).writebuf_last as usize].data = v;
    time1 = (*chip).writebuf_lasttime + OPL_WRITEBUF_DELAY;
    time2 = (*chip).writebuf_samplecnt;

    if time1 < time2 {
        time1 = time2;
    }

    (*chip).writebuf[(*chip).writebuf_last as usize].time = time1;
    (*chip).writebuf_lasttime = time1;
    (*chip).writebuf_last = ((*chip).writebuf_last + 1) % OPL_WRITEBUF_SIZE as u32;
}

#[no_mangle]
pub unsafe extern "C" fn OPL3_GenerateStream(
    chip: *mut opl3_chip,
    mut sndptr: *mut i16,
    numsamples: u32,
) {
    let mut i: u32 = 0;
    while i < numsamples {
        OPL3_GenerateResampled(chip, sndptr);
        sndptr = sndptr.add(2);
        i += 1;
    }
}
