use alloc::vec::Vec;

use bytes::BufMut;

use crate::buffer::{BufExt, SafeBuf};

use super::{triplet::COORD_LUT, GlyfDecoderError, Woff2GlyfDecoder};

impl Woff2GlyfDecoder<'_> {
    fn take_point_counts(
        &mut self,
        number_of_contours: i16,
    ) -> Result<(Vec<u16>, u16), GlyfDecoderError> {
        let mut n_points_stream = self.n_points_stream;
        let mut point_counts = Vec::with_capacity(number_of_contours as usize);
        let mut total_points = 0u16;

        for _ in 0..number_of_contours {
            let number_of_points = n_points_stream.try_get_255_u16()?;
            total_points += number_of_points;
            point_counts.push(number_of_points);
        }

        self.n_points_stream = n_points_stream;
        Ok((point_counts, total_points))
    }

    pub(super) fn parse_simple_glyph(
        &mut self,
        number_of_contours: i16,
        glyph_index: u16,
        output_buffer: &mut Vec<u8>,
    ) -> Result<(), GlyfDecoderError> {
        let (point_counts, total_points) = self.take_point_counts(number_of_contours)?;

        let mut end_points_of_contours_stream =
            Vec::with_capacity(number_of_contours as usize * core::mem::size_of::<u16>());
        let mut instructions_stream = Vec::new();
        let mut flags_stream = Vec::with_capacity(total_points as usize);
        let mut x_coordinates_stream = Vec::with_capacity(total_points as usize * 2);
        let mut y_coordinates_stream = Vec::with_capacity(total_points as usize * 2);

        let mut running_total_points: u16 = 0;
        let overlap_simple_flag = match self.overlap_bitmap {
            Some(ob) if ob[glyph_index as usize] => 0x40,
            _ => 0x00,
        };

        let mut x_min = 0i16;
        let mut y_min = 0i16;
        let mut x_max = 0i16;
        let mut y_max = 0i16;
        let mut extents_set = false;
        let mut x = 0i16;
        let mut y = 0i16;

        for number_of_points in point_counts {
            running_total_points += number_of_points;
            end_points_of_contours_stream.put_u16(running_total_points - 1);
            for _ in 0..number_of_points {
                let flags = SafeBuf::try_get_u8(&mut self.flag_stream)?;
                let triplet = &COORD_LUT[(flags & 0x7f) as usize];
                let data = match triplet.byte_count {
                    1 => SafeBuf::try_get_u8(&mut self.glyph_stream)? as u32,
                    2 => SafeBuf::try_get_u16(&mut self.glyph_stream)? as u32,
                    3 => {
                        ((SafeBuf::try_get_u8(&mut self.glyph_stream)? as u32) << 16)
                            | (SafeBuf::try_get_u16(&mut self.glyph_stream)? as u32)
                    }
                    4 => SafeBuf::try_get_u32(&mut self.glyph_stream)?,
                    _ => panic!(),
                };
                let dx = triplet.dx(data);
                let dy = triplet.dy(data);
                x += dx;
                y += dy;
                if extents_set {
                    x_min = x_min.min(x);
                    y_min = y_min.min(y);
                    x_max = x_max.max(x);
                    y_max = y_max.max(y);
                } else {
                    x_min = x;
                    x_max = x;
                    y_min = y;
                    y_max = y;
                    extents_set = true;
                }

                let point_is_on_curve = (flags & 0x80) == 0x00;
                let on_curve_flag = if point_is_on_curve { 0x01 } else { 0x00 };
                let (x_short_vector_flag, x_is_same_flag) = match dx {
                    0 => (0x00, 0x10),
                    1..=255 => {
                        x_coordinates_stream.put_u8(u8::try_from(dx).unwrap());
                        (0x02, 0x10)
                    }
                    -255..=-1 => {
                        x_coordinates_stream.put_u8(u8::try_from(-dx).unwrap());
                        (0x02, 0x00)
                    }
                    _ => {
                        x_coordinates_stream.put_i16(dx);
                        (0x00, 0x00)
                    }
                };
                let (y_short_vector_flag, y_is_same_flag) = match dy {
                    0 => (0x00, 0x20),
                    1..=255 => {
                        y_coordinates_stream.put_u8(u8::try_from(dy).unwrap());
                        (0x04, 0x20)
                    }
                    -255..=-1 => {
                        y_coordinates_stream.put_u8(u8::try_from(-dy).unwrap());
                        (0x04, 0x00)
                    }
                    _ => {
                        y_coordinates_stream.put_i16(dy);
                        (0x00, 0x00)
                    }
                };

                flags_stream.put_u8(
                    on_curve_flag
                        | x_short_vector_flag
                        | y_short_vector_flag
                        | x_is_same_flag
                        | y_is_same_flag
                        | overlap_simple_flag,
                );
            }
        }

        let instruction_length = self.glyph_stream.try_get_255_u16()?;
        instructions_stream.reserve_exact(instruction_length as usize);
        self.instruction_stream
            .try_copy_to_buf(&mut instructions_stream, instruction_length as usize)?;

        if self.bbox_bitmap[glyph_index as usize] {
            x_min = SafeBuf::try_get_i16(&mut self.bbox_stream)?;
            y_min = SafeBuf::try_get_i16(&mut self.bbox_stream)?;
            x_max = SafeBuf::try_get_i16(&mut self.bbox_stream)?;
            y_max = SafeBuf::try_get_i16(&mut self.bbox_stream)?;
        }

        output_buffer.put_i16(number_of_contours);
        output_buffer.put_i16(x_min);
        output_buffer.put_i16(y_min);
        output_buffer.put_i16(x_max);
        output_buffer.put_i16(y_max);
        output_buffer.extend_from_slice(&end_points_of_contours_stream);
        output_buffer.put_u16(instruction_length);
        output_buffer.extend_from_slice(&instructions_stream);
        output_buffer.extend_from_slice(&flags_stream);
        output_buffer.extend_from_slice(&x_coordinates_stream);
        output_buffer.extend_from_slice(&y_coordinates_stream);

        Ok(())
    }
}
