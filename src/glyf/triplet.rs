// Copied from the Allsorts Rust package
// https://github.com/yeslogic/allsorts/blob/master/src/woff2/lut.rs
//
// Copyright 2019 YesLogic Pty. Ltd. <info@yeslogic.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[derive(Clone, Copy)]
pub struct XYTriplet {
    pub x_is_negative: bool,
    pub y_is_negative: bool,
    pub byte_count: u8,
    pub x_bits: u8,
    pub y_bits: u8,
    pub delta_x: u16,
    pub delta_y: u16,
}

impl XYTriplet {
    const fn new(
        byte_count: u8,
        x_bits: u8,
        y_bits: u8,
        delta_x: u16,
        delta_y: u16,
        x_is_negative: bool,
        y_is_negative: bool,
    ) -> Self {
        Self {
            x_is_negative,
            y_is_negative,
            byte_count,
            x_bits,
            y_bits,
            delta_x,
            delta_y,
        }
    }

    pub fn dx(&self, data: u32) -> i16 {
        let mask = (1u32 << self.x_bits) - 1;
        let shift = (self.byte_count * 8) - self.x_bits;
        let dx = ((data >> shift) & mask) + u32::from(self.delta_x);

        if self.x_is_negative {
            -(dx as i16)
        } else {
            dx as i16
        }
    }

    pub fn dy(&self, data: u32) -> i16 {
        let mask = (1u32 << self.y_bits) - 1;
        let shift = (self.byte_count * 8) - self.x_bits - self.y_bits;
        let dy = ((data >> shift) & mask) + u32::from(self.delta_y);

        if self.y_is_negative {
            -(dy as i16)
        } else {
            dy as i16
        }
    }
}

// Lookup table for decoding transformed glyf table point coordinates
// https://www.w3.org/TR/WOFF2/#glyf_table_format
const DELTA_4BIT: [u16; 4] = [1, 17, 33, 49];
const DELTA_8BIT: [u16; 3] = [1, 257, 513];

pub static COORD_LUT: [XYTriplet; 128] = {
    let mut lut = [XYTriplet::new(0, 0, 0, 0, 0, false, false); 128];
    let mut index = 0;

    while index < lut.len() {
        lut[index] = coord_lut_entry(index);
        index += 1;
    }

    lut
};

const fn coord_lut_entry(index: usize) -> XYTriplet {
    match index {
        0..=9 => y_only_entry(index),
        10..=19 => x_only_entry(index - 10),
        20..=83 => packed_4bit_entry(index - 20),
        84..=119 => packed_8bit_entry(index - 84),
        120..=123 => signed_entry(3, 12, 12, 0, 0, index - 120),
        124..=127 => signed_entry(4, 16, 16, 0, 0, index - 124),
        _ => unreachable!(),
    }
}

const fn y_only_entry(index: usize) -> XYTriplet {
    let delta_y = ((index / 2) as u16) * 256;
    let y_is_negative = index.is_multiple_of(2);
    XYTriplet::new(1, 0, 8, 0, delta_y, false, y_is_negative)
}

const fn x_only_entry(index: usize) -> XYTriplet {
    let delta_x = ((index / 2) as u16) * 256;
    let x_is_negative = index.is_multiple_of(2);
    XYTriplet::new(1, 8, 0, delta_x, 0, x_is_negative, false)
}

const fn packed_4bit_entry(index: usize) -> XYTriplet {
    let x_group = index / 16;
    let y_group = (index / 4) % 4;
    let delta_x = DELTA_4BIT[x_group];
    let delta_y = DELTA_4BIT[y_group];
    let sign_group = index % 4;
    signed_entry(1, 4, 4, delta_x, delta_y, sign_group)
}

const fn packed_8bit_entry(index: usize) -> XYTriplet {
    let x_group = index / 12;
    let y_group = (index / 4) % 3;
    let delta_x = DELTA_8BIT[x_group];
    let delta_y = DELTA_8BIT[y_group];
    let sign_group = index % 4;
    signed_entry(2, 8, 8, delta_x, delta_y, sign_group)
}

const fn signed_entry(
    count: u8,
    x_bits: u8,
    y_bits: u8,
    delta_x: u16,
    delta_y: u16,
    sign_group: usize,
) -> XYTriplet {
    let x_neg = sign_group & 0b01 == 0;
    let y_neg = sign_group & 0b10 == 0;
    XYTriplet::new(count, x_bits, y_bits, delta_x, delta_y, x_neg, y_neg)
}
