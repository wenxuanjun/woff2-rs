//! Interface for decoding WOFF2 files

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use bytes::Buf;
use thiserror::Error;

use crate::brotli::decompress_to_vec;
use crate::checksum::ChecksumError;
use crate::checksum::{calculate_font_checksum_adjustment, set_checksum_adjustment};
use crate::magic::*;
use crate::sfnt::{calculate_header_size, TableDirectory};
use crate::woff2::collection::{CollectionHeader, CollectionHeaderError};
use crate::woff2::header::{Woff2Header, Woff2HeaderError};
use crate::woff2::tables::{TableDirectoryError, Woff2TableDirectory};
use crate::woff2::tables::{WriteTablesError, HEAD_TAG};

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Invalid Woff2 File {0}")]
    Invalid(String),
    #[error("Unsupported feature {0}")]
    Unsupported(&'static str),
}

impl From<ChecksumError> for DecodeError {
    fn from(e: ChecksumError) -> Self {
        DecodeError::invalid(e)
    }
}

impl From<CollectionHeaderError> for DecodeError {
    fn from(e: CollectionHeaderError) -> Self {
        DecodeError::invalid(e)
    }
}

impl From<TableDirectoryError> for DecodeError {
    fn from(e: TableDirectoryError) -> Self {
        DecodeError::invalid(e)
    }
}

impl From<Woff2HeaderError> for DecodeError {
    fn from(e: Woff2HeaderError) -> Self {
        DecodeError::invalid(e)
    }
}

impl From<WriteTablesError> for DecodeError {
    fn from(e: WriteTablesError) -> Self {
        match e {
            WriteTablesError::Unsupported(e) => DecodeError::Unsupported(e),
            _ => DecodeError::invalid(e),
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for DecodeError {
    fn from(error: std::io::Error) -> Self {
        DecodeError::invalid(error)
    }
}

impl From<&'static str> for DecodeError {
    fn from(e: &'static str) -> Self {
        DecodeError::Invalid(e.into())
    }
}

impl DecodeError {
    fn invalid<E: ToString>(error: E) -> Self {
        Self::Invalid(error.to_string())
    }
}

/// Returns whether the buffer starts with the WOFF2 magic number.
pub fn is_woff2(input_buffer: &[u8]) -> bool {
    input_buffer.starts_with(&WOFF2_SIGNATURE.0)
}

/// Converts a WOFF2 font in `input_buffer` into a TTF format font.
pub fn convert_woff2_to_ttf(input_buffer: &mut impl Buf) -> Result<Vec<u8>, DecodeError> {
    let header = Woff2Header::from_buf(input_buffer)?;
    header.is_valid_header()?;

    if !matches!(
        header.flavor,
        TTF_COLLECTION_FLAVOR | TTF_CFF_FLAVOR | TTF_TRUE_TYPE_FLAVOR
    ) {
        Err(DecodeError::Invalid("Invalid font flavor".into()))?;
    }

    let table_directory = Woff2TableDirectory::from_buf(input_buffer, header.num_tables)?;

    let mut collection_header = if header.flavor == TTF_COLLECTION_FLAVOR {
        Some(CollectionHeader::from_buf(input_buffer, header.num_tables)?)
    } else {
        None
    };

    let compressed_stream_size = usize::try_from(header.total_compressed_size).unwrap();
    if input_buffer.remaining() < compressed_stream_size {
        Err(DecodeError::Invalid("Truncated compressed stream".into()))?;
    }

    let mut compressed_stream = (&mut *input_buffer).take(compressed_stream_size);
    let mut decompressed_tables = Vec::with_capacity(table_directory.uncompressed_length as usize);
    decompress_to_vec(&mut compressed_stream, &mut decompressed_tables)?;

    if compressed_stream.remaining() != 0 {
        Err(DecodeError::Invalid(
            "Compressed stream size does not match header".into(),
        ))?;
    }

    let mut out_buffer = Vec::with_capacity(header.total_sfnt_size as usize);
    let header_end = if let Some(collection_header) = &collection_header {
        collection_header.calculate_header_size()
    } else {
        calculate_header_size(table_directory.tables.len())
    };
    out_buffer.resize(header_end, 0);
    let ttf_tables = table_directory.write_to_buf(&mut out_buffer, &decompressed_tables)?;

    let mut header_buffer = &mut out_buffer[..header_end];
    if let Some(collection_header) = &mut collection_header {
        for font in &mut collection_header.fonts {
            font.table_indices
                .sort_unstable_by_key(|&idx| ttf_tables[idx as usize].tag.0);
        }
        collection_header.write_to_buf(&mut header_buffer, &ttf_tables);
    } else {
        let ttf_header = TableDirectory::new(header.flavor, ttf_tables);
        ttf_header.write_to_buf(&mut header_buffer);
        let head_table_record = ttf_header
            .find_table(HEAD_TAG)
            .ok_or_else(|| DecodeError::Invalid("Missing `head` table".into()))?;
        let checksum_adjustment = calculate_font_checksum_adjustment(&out_buffer);
        let head_table = &mut out_buffer[head_table_record.get_range()];
        set_checksum_adjustment(head_table, checksum_adjustment)?;
    }

    Ok(out_buffer)
}

#[cfg(test)]
mod tests {
    use crate::test_data::{FONTAWESOME_REGULAR_400, LATO_V22_LATIN_REGULAR};

    use super::convert_woff2_to_ttf;

    #[test]
    fn read_sample_font() {
        let buffer = LATO_V22_LATIN_REGULAR;
        let ttf = convert_woff2_to_ttf(&mut &buffer[..]).unwrap();
        assert_eq!(None, ttf_parser::fonts_in_collection(&ttf));
        let _parsed_ttf = ttf_parser::Face::parse(&ttf, 0).unwrap();
    }

    #[test]
    fn read_loca_is_not_after_glyf_font() {
        let buffer = FONTAWESOME_REGULAR_400;
        let ttf = convert_woff2_to_ttf(&mut &buffer[..]).unwrap();
        assert_eq!(None, ttf_parser::fonts_in_collection(&ttf));
        let _parsed_ttf = ttf_parser::Face::parse(&ttf, 0).unwrap();
    }

    #[test]
    fn sample_font_is_woff2() {
        assert!(super::is_woff2(LATO_V22_LATIN_REGULAR));
    }
}
