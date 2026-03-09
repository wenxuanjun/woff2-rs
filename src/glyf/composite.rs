use alloc::vec::Vec;

use bytes::BufMut;

use crate::buffer::{BufExt, SafeBuf};

use super::{GlyfDecoderError, Woff2GlyfDecoder};

impl Woff2GlyfDecoder<'_> {
    pub(super) fn parse_composite_glyph(
        &mut self,
        glyph_index: u16,
        output_buffer: &mut Vec<u8>,
    ) -> Result<(), GlyfDecoderError> {
        output_buffer.put_i16(-1);
        if self.bbox_bitmap[glyph_index as usize] {
            output_buffer.put_i16(SafeBuf::try_get_i16(&mut self.bbox_stream)?);
            output_buffer.put_i16(SafeBuf::try_get_i16(&mut self.bbox_stream)?);
            output_buffer.put_i16(SafeBuf::try_get_i16(&mut self.bbox_stream)?);
            output_buffer.put_i16(SafeBuf::try_get_i16(&mut self.bbox_stream)?);
        } else {
            Err(GlyfDecoderError::CompositeGlyphWithoutBbox)?
        }

        let mut have_instructions = false;
        loop {
            let flag_word = SafeBuf::try_get_u16(&mut self.composite_stream)?;
            let mut num_bytes = 4usize;

            if flag_word & 0x0001 == 0x0001 {
                num_bytes += 2;
            }
            if flag_word & 0x0008 == 0x0008 {
                num_bytes += 2;
            } else if flag_word & 0x0040 == 0x0040 {
                num_bytes += 4;
            } else if flag_word & 0x0080 == 0x0080 {
                num_bytes += 8;
            }

            output_buffer.put_u16(flag_word);
            self.composite_stream
                .try_copy_to_buf(output_buffer, num_bytes)?;

            if flag_word & 0x0100 == 0x0100 {
                have_instructions = true;
            }

            if flag_word & 0x0020 == 0 {
                break;
            }
        }

        if have_instructions {
            let instruction_length = self.glyph_stream.try_get_255_u16()?;
            output_buffer.put_u16(instruction_length);
            self.instruction_stream
                .try_copy_to_buf(output_buffer, instruction_length as usize)?;
        }

        Ok(())
    }
}
