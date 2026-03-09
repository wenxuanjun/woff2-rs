use alloc::vec::Vec;
use bitvec::{order::Msb0, slice::BitSlice};
use bytes::{Buf, BufMut};
use thiserror::Error;

use crate::buffer::{pad_to_multiple_of_four, SafeBuf};

mod composite;
mod simple;
mod triplet;

pub fn decode_glyf_table(glyf_table: &[u8]) -> Result<(Vec<u8>, Vec<u8>), GlyfDecoderError> {
    let mut decoder = Woff2GlyfDecoder::new(glyf_table)?;
    let res = decoder.parse_all_glyphs()?;
    if decoder.has_read_all() {
        Ok(res)
    } else {
        Err(GlyfDecoderError::ExtraData)
    }
}

#[derive(Error, Debug)]
pub enum GlyfDecoderError {
    #[error("Stream truncated")]
    Truncated,
    #[error("Composite glyph without bbox")]
    CompositeGlyphWithoutBbox,
    #[error("Extra Data")]
    ExtraData,
}

impl From<crate::buffer::TruncatedError> for GlyfDecoderError {
    fn from(_: crate::buffer::TruncatedError) -> Self {
        GlyfDecoderError::Truncated
    }
}

struct Woff2GlyfDecoder<'a> {
    num_glyphs: u16,
    n_contour_stream: &'a [u8],
    n_points_stream: &'a [u8],
    flag_stream: &'a [u8],
    glyph_stream: &'a [u8],
    composite_stream: &'a [u8],
    bbox_bitmap: &'a BitSlice<u8, Msb0>,
    bbox_stream: &'a [u8],
    instruction_stream: &'a [u8],
    overlap_bitmap: Option<&'a BitSlice<u8, Msb0>>,
    index_format: u16,
}

fn bit_stream_byte_length(bit_stream_bit_length: u16) -> u16 {
    ((bit_stream_bit_length >> 5)
        + if !bit_stream_bit_length.is_multiple_of(32) {
            1
        } else {
            0
        })
        << 2
}

impl<'a> Woff2GlyfDecoder<'a> {
    fn has_read_all(&self) -> bool {
        self.n_contour_stream.remaining() == 0
            && self.n_points_stream.remaining() == 0
            && self.flag_stream.remaining() == 0
            && self.glyph_stream.remaining() == 0
            && self.composite_stream.remaining() == 0
            && self.bbox_stream.remaining() == 0
            && self.instruction_stream.remaining() == 0
    }

    fn new(transformed_glyf_table: &'a [u8]) -> Result<Self, GlyfDecoderError> {
        let mut table_buf = transformed_glyf_table;

        const GLYF_HEADER_SIZE: usize = 36;
        if table_buf.remaining() < GLYF_HEADER_SIZE {
            return Err(GlyfDecoderError::Truncated);
        }
        let _ = table_buf.get_u16();
        let option_flags = table_buf.get_u16();
        let num_glyphs = table_buf.get_u16();
        let bitmap_stream_length = bit_stream_byte_length(num_glyphs);
        let index_format = table_buf.get_u16();
        let n_contour_stream_size = table_buf.get_u32();
        let n_points_stream_size = table_buf.get_u32();
        let flag_stream_size = table_buf.get_u32();
        let glyph_stream_size = table_buf.get_u32();
        let composite_stream_size = table_buf.get_u32();
        let bbox_bitmap_size = bitmap_stream_length;
        let bbox_stream_size = table_buf.get_u32() - bbox_bitmap_size as u32;
        let instruction_stream_size = table_buf.get_u32();
        let header_end = transformed_glyf_table.len() - table_buf.remaining();
        assert_eq!(header_end, GLYF_HEADER_SIZE);
        let has_overlap_bit_stream = (option_flags & 0x01) == 0x01;
        let overlap_simple_bit_stream_size = if has_overlap_bit_stream {
            bit_stream_byte_length(num_glyphs)
        } else {
            0
        };

        let n_contour_stream_start = header_end;
        let n_points_stream_start = n_contour_stream_start + n_contour_stream_size as usize;
        let flag_stream_start = n_points_stream_start + n_points_stream_size as usize;
        let glyph_stream_start = flag_stream_start + flag_stream_size as usize;
        let composite_stream_start = glyph_stream_start + glyph_stream_size as usize;
        let bbox_bitmap_start = composite_stream_start + composite_stream_size as usize;
        let bbox_stream_start = bbox_bitmap_start + bbox_bitmap_size as usize;
        let instruction_stream_start = bbox_stream_start + bbox_stream_size as usize;
        let overlap_bit_stream_start = instruction_stream_start + instruction_stream_size as usize;
        let overlap_bit_stream_end =
            overlap_bit_stream_start + overlap_simple_bit_stream_size as usize;
        if transformed_glyf_table.len() < overlap_bit_stream_end {
            return Err(GlyfDecoderError::Truncated);
        }

        Ok(Self {
            num_glyphs,
            n_contour_stream: &transformed_glyf_table
                [n_contour_stream_start..n_points_stream_start],
            n_points_stream: &transformed_glyf_table[n_points_stream_start..flag_stream_start],
            flag_stream: &transformed_glyf_table[flag_stream_start..glyph_stream_start],
            glyph_stream: &transformed_glyf_table[glyph_stream_start..composite_stream_start],
            composite_stream: &transformed_glyf_table[composite_stream_start..bbox_bitmap_start],
            bbox_bitmap: BitSlice::<_, Msb0>::from_slice(
                &transformed_glyf_table[bbox_bitmap_start..bbox_stream_start],
            ),
            bbox_stream: &transformed_glyf_table[bbox_stream_start..instruction_stream_start],
            instruction_stream: &transformed_glyf_table
                [instruction_stream_start..overlap_bit_stream_start],
            overlap_bitmap: if has_overlap_bit_stream {
                Some(BitSlice::<_, Msb0>::from_slice(
                    &transformed_glyf_table[overlap_bit_stream_start
                        ..overlap_bit_stream_start + overlap_simple_bit_stream_size as usize],
                ))
            } else {
                None
            },
            index_format,
        })
    }

    fn parse_next_glyph(
        &mut self,
        glyph_index: u16,
        output_vector: &mut Vec<u8>,
    ) -> Result<(), GlyfDecoderError> {
        let number_of_contours = SafeBuf::try_get_i16(&mut self.n_contour_stream)?;
        match number_of_contours {
            0 => Ok(()),
            num if num > 0 => {
                self.parse_simple_glyph(number_of_contours, glyph_index, output_vector)
            }
            _ => self.parse_composite_glyph(glyph_index, output_vector),
        }
    }

    fn parse_all_glyphs(&mut self) -> Result<(Vec<u8>, Vec<u8>), GlyfDecoderError> {
        let loca_use_u32 = self.index_format > 0;
        let loca_capacity = (self.num_glyphs + 1) as usize * if loca_use_u32 { 4 } else { 2 };
        let mut output_glyf_table = Vec::new();
        let mut output_loca_table = Vec::with_capacity(loca_capacity);
        for glyph_index in 0..self.num_glyphs {
            if loca_use_u32 {
                output_loca_table.put_u32(output_glyf_table.len().try_into().unwrap());
            } else {
                output_loca_table.put_u16((output_glyf_table.len() / 2).try_into().unwrap());
            }
            self.parse_next_glyph(glyph_index, &mut output_glyf_table)?;
            pad_to_multiple_of_four(&mut output_glyf_table);
        }
        if loca_use_u32 {
            output_loca_table.put_u32(output_glyf_table.len().try_into().unwrap());
        } else {
            if output_glyf_table.len() % 2 == 1 {
                output_glyf_table.put_u8(0);
            }
            output_loca_table.put_u16((output_glyf_table.len() / 2).try_into().unwrap());
        }
        Ok((output_glyf_table, output_loca_table))
    }
}
