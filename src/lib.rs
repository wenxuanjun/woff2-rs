#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod decode;

mod buffer_util;
mod checksum;
mod glyf;
mod magic_numbers;
mod ttf_header;
mod woff2;

#[cfg(test)]
mod test_data;

pub use decode::convert_woff2_to_ttf;
