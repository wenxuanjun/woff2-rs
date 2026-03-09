#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod decode;

mod brotli;
mod buffer;
mod checksum;
mod glyf;
mod magic;
mod sfnt;
mod woff2;

#[cfg(test)]
mod test_data;

pub use decode::convert_woff2_to_ttf;
