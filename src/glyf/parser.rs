use alloc::vec::Vec;

use bytes::Buf;

use super::{Bitmap, GlyfDecoderError, Woff2GlyfDecoder};

const GLYF_HEADER_SIZE: usize = 36;
const OVERLAP_BITMAP_FLAG: u16 = 0x01;

struct GlyfHeader {
    num_glyphs: u16,
    index_format: u16,
    n_contour_stream_size: usize,
    n_points_stream_size: usize,
    flag_stream_size: usize,
    glyph_stream_size: usize,
    composite_stream_size: usize,
    bbox_bitmap_size: usize,
    bbox_stream_size: usize,
    instruction_stream_size: usize,
    overlap_bitmap_size: usize,
}

struct StreamReader<'a> {
    input: &'a [u8],
}

pub(crate) fn decode_glyf_table(glyf_table: &[u8]) -> Result<(Vec<u8>, Vec<u8>), GlyfDecoderError> {
    let mut decoder = Woff2GlyfDecoder::new(glyf_table)?;
    let res = decoder.parse_all_glyphs()?;
    if decoder.has_read_all() {
        Ok(res)
    } else {
        Err(GlyfDecoderError::ExtraData)
    }
}

impl GlyfHeader {
    fn parse(transformed_glyf_table: &[u8]) -> Result<Self, GlyfDecoderError> {
        let mut table_buf = transformed_glyf_table;
        if table_buf.remaining() < GLYF_HEADER_SIZE {
            return Err(GlyfDecoderError::Truncated);
        }

        let _reserved = table_buf.get_u16();
        let option_flags = table_buf.get_u16();
        let num_glyphs = table_buf.get_u16();
        let bbox_bitmap_size = bit_stream_byte_length(num_glyphs);
        let index_format = table_buf.get_u16();
        let n_contour_stream_size = table_buf.get_u32() as usize;
        let n_points_stream_size = table_buf.get_u32() as usize;
        let flag_stream_size = table_buf.get_u32() as usize;
        let glyph_stream_size = table_buf.get_u32() as usize;
        let composite_stream_size = table_buf.get_u32() as usize;
        let bbox_stream_size = (table_buf.get_u32() as usize)
            .checked_sub(bbox_bitmap_size)
            .ok_or(GlyfDecoderError::Truncated)?;
        let instruction_stream_size = table_buf.get_u32() as usize;
        let overlap_bitmap_size = if option_flags & OVERLAP_BITMAP_FLAG != 0 {
            bit_stream_byte_length(num_glyphs)
        } else {
            0
        };

        Ok(Self {
            num_glyphs,
            index_format,
            n_contour_stream_size,
            n_points_stream_size,
            flag_stream_size,
            glyph_stream_size,
            composite_stream_size,
            bbox_bitmap_size,
            bbox_stream_size,
            instruction_stream_size,
            overlap_bitmap_size,
        })
    }

    fn has_overlap_bitmap(&self) -> bool {
        self.overlap_bitmap_size != 0
    }
}

impl<'a> StreamReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], GlyfDecoderError> {
        if self.input.len() < len {
            return Err(GlyfDecoderError::Truncated);
        }

        let (stream, rest) = self.input.split_at(len);
        self.input = rest;
        Ok(stream)
    }

    fn finish(self) -> &'a [u8] {
        self.input
    }
}

impl<'a> Woff2GlyfDecoder<'a> {
    pub(super) fn new(transformed_glyf_table: &'a [u8]) -> Result<Self, GlyfDecoderError> {
        let header = GlyfHeader::parse(transformed_glyf_table)?;
        let mut streams = StreamReader::new(&transformed_glyf_table[GLYF_HEADER_SIZE..]);

        let n_contour_stream = streams.take(header.n_contour_stream_size)?;
        let n_points_stream = streams.take(header.n_points_stream_size)?;
        let flag_stream = streams.take(header.flag_stream_size)?;
        let glyph_stream = streams.take(header.glyph_stream_size)?;
        let composite_stream = streams.take(header.composite_stream_size)?;
        let bbox_bitmap = Bitmap::new(streams.take(header.bbox_bitmap_size)?);
        let bbox_stream = streams.take(header.bbox_stream_size)?;
        let instruction_stream = streams.take(header.instruction_stream_size)?;
        let overlap_bitmap = if header.has_overlap_bitmap() {
            Some(Bitmap::new(streams.take(header.overlap_bitmap_size)?))
        } else {
            None
        };

        Ok(Self {
            num_glyphs: header.num_glyphs,
            n_contour_stream,
            n_points_stream,
            flag_stream,
            glyph_stream,
            composite_stream,
            bbox_bitmap,
            bbox_stream,
            instruction_stream,
            overlap_bitmap,
            trailer: streams.finish(),
            index_format: header.index_format,
        })
    }
}

fn bit_stream_byte_length(bit_stream_bit_length: u16) -> usize {
    usize::from(
        ((bit_stream_bit_length >> 5)
            + if !bit_stream_bit_length.is_multiple_of(32) {
                1
            } else {
                0
            })
            << 2,
    )
}
