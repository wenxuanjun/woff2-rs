use std::io::Cursor;

use bitvec::{order::Msb0, slice::BitSlice};
use bytes::{Buf, BufMut};
use safer_bytes::{error::Truncated, SafeBuf};
use thiserror::Error;

use crate::buffer_util::pad_to_multiple_of_four;

mod composite;
mod simple;
mod triplet;

#[derive(Error, Debug)]
pub enum GlyfDecoderError {
    #[error("Stream truncated")]
    Truncated,
    #[error("Composite glyph without bbox")]
    CompositeGlyphWithoutBbox,
    #[error("Extra Data")]
    ExtraData,
}

impl From<Truncated> for GlyfDecoderError {
    fn from(_: Truncated) -> Self {
        GlyfDecoderError::Truncated
    }
}

impl From<std::io::Error> for GlyfDecoderError {
    fn from(_: std::io::Error) -> Self {
        GlyfDecoderError::Truncated
    }
}

struct Woff2GlyfDecoder<'a, T> {
    num_glyphs: u16,
    n_contour_stream: Cursor<T>,
    n_points_stream: Cursor<T>,
    flag_stream: Cursor<T>,
    glyph_stream: Cursor<T>,
    composite_stream: Cursor<T>,
    bbox_bitmap: &'a BitSlice<u8, Msb0>,
    bbox_stream: Cursor<T>,
    instruction_stream: Cursor<T>,
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

impl<'a> Woff2GlyfDecoder<'a, &'a [u8]> {
    fn has_read_all(&self) -> bool {
        let _n_contour_stream_remaining = self.n_contour_stream.remaining();
        let _n_points_stream_reminaing = self.n_points_stream.remaining();

        self.n_contour_stream.remaining() == 0
            && self.n_points_stream.remaining() == 0
            && self.flag_stream.remaining() == 0
            && self.glyph_stream.remaining() == 0
            && self.composite_stream.remaining() == 0
            && self.bbox_stream.remaining() == 0
            && self.instruction_stream.remaining() == 0
    }

    fn new(transformed_glyf_table: &'a [u8]) -> Result<Self, GlyfDecoderError> {
        let mut table_buf = Cursor::new(transformed_glyf_table);

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
        assert_eq!(table_buf.position() as usize, GLYF_HEADER_SIZE);
        let has_overlap_bit_stream = (option_flags & 0x01) == 0x01;
        let overlap_simple_bit_stream_size = if has_overlap_bit_stream {
            bit_stream_byte_length(num_glyphs)
        } else {
            0
        };

        let n_contour_stream_start: usize = table_buf.position().try_into().unwrap();
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
        let n_contour_stream =
            Cursor::new(&transformed_glyf_table[n_contour_stream_start..n_points_stream_start]);
        let n_points_stream =
            Cursor::new(&transformed_glyf_table[n_points_stream_start..flag_stream_start]);
        let flag_stream =
            Cursor::new(&transformed_glyf_table[flag_stream_start..glyph_stream_start]);
        let glyph_stream =
            Cursor::new(&transformed_glyf_table[glyph_stream_start..composite_stream_start]);
        let composite_stream =
            Cursor::new(&transformed_glyf_table[composite_stream_start..bbox_bitmap_start]);
        let bbox_bitmap = BitSlice::<_, Msb0>::from_slice(
            &transformed_glyf_table[bbox_bitmap_start..bbox_stream_start],
        );
        let bbox_stream =
            Cursor::new(&transformed_glyf_table[bbox_stream_start..instruction_stream_start]);
        let instruction_stream = Cursor::new(
            &transformed_glyf_table[instruction_stream_start..overlap_bit_stream_start],
        );
        let overlap_bitmap = if has_overlap_bit_stream {
            Some(BitSlice::<_, Msb0>::from_slice(
                &transformed_glyf_table[overlap_bit_stream_start
                    ..overlap_bit_stream_start + overlap_simple_bit_stream_size as usize],
            ))
        } else {
            None
        };

        Ok(Self {
            num_glyphs,
            n_contour_stream,
            n_points_stream,
            flag_stream,
            glyph_stream,
            composite_stream,
            bbox_bitmap,
            bbox_stream,
            instruction_stream,
            overlap_bitmap,
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
        let mut output_glyf_table: Vec<u8> = Vec::new();
        let mut output_loca_table: Vec<u8> = Vec::with_capacity(loca_capacity);
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

pub fn decode_glyf_table(glyf_table: &[u8]) -> Result<(Vec<u8>, Vec<u8>), GlyfDecoderError> {
    let mut decoder = Woff2GlyfDecoder::new(glyf_table)?;
    let res = decoder.parse_all_glyphs()?;
    if decoder.has_read_all() {
        Ok(res)
    } else {
        Err(GlyfDecoderError::ExtraData)
    }
}
