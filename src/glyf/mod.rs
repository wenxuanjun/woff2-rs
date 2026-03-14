use alloc::vec::Vec;
use bytes::BufMut;
use thiserror::Error;

use crate::buffer::{pad_to_multiple_of_four, SafeBuf};

mod bitmap;
mod composite;
mod parser;
mod simple;
mod triplet;

use bitmap::Bitmap;
pub(crate) use parser::decode_glyf_table;

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
    bbox_bitmap: Bitmap<'a>,
    bbox_stream: &'a [u8],
    instruction_stream: &'a [u8],
    overlap_bitmap: Option<Bitmap<'a>>,
    trailer: &'a [u8],
    index_format: u16,
}

impl<'a> Woff2GlyfDecoder<'a> {
    fn has_read_all(&self) -> bool {
        self.n_contour_stream.is_empty()
            && self.n_points_stream.is_empty()
            && self.flag_stream.is_empty()
            && self.glyph_stream.is_empty()
            && self.composite_stream.is_empty()
            && self.bbox_stream.is_empty()
            && self.instruction_stream.is_empty()
            && self.trailer.is_empty()
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
        let loca_uses_u32 = self.index_format > 0;
        let loca_entry_size = if loca_uses_u32 { 4 } else { 2 };
        let mut output_glyf_table = Vec::new();
        let mut output_loca_table =
            Vec::with_capacity(usize::from(self.num_glyphs + 1) * loca_entry_size);

        let write_loca_entry = |loca_table: &mut Vec<u8>, glyf_len: usize| {
            if loca_uses_u32 {
                loca_table.put_u32(glyf_len.try_into().unwrap());
            } else {
                loca_table.put_u16((glyf_len / 2).try_into().unwrap());
            }
        };

        for glyph_index in 0..self.num_glyphs {
            write_loca_entry(&mut output_loca_table, output_glyf_table.len());
            self.parse_next_glyph(glyph_index, &mut output_glyf_table)?;
            pad_to_multiple_of_four(&mut output_glyf_table);
        }

        if !loca_uses_u32 && output_glyf_table.len() % 2 == 1 {
            output_glyf_table.put_u8(0);
        }
        write_loca_entry(&mut output_loca_table, output_glyf_table.len());

        Ok((output_glyf_table, output_loca_table))
    }
}
