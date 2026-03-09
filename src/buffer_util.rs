use alloc::vec::Vec;
use bytes::{Buf, BufMut};
use four_cc::FourCC;
use thiserror::Error;

#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("buffer truncated")]
pub struct TruncatedError;

pub trait SafeBuf: Buf {
    fn try_get_u8(&mut self) -> Result<u8, TruncatedError> {
        if self.remaining() < 1 {
            Err(TruncatedError)
        } else {
            Ok(self.get_u8())
        }
    }

    fn try_get_u16(&mut self) -> Result<u16, TruncatedError> {
        if self.remaining() < 2 {
            Err(TruncatedError)
        } else {
            Ok(self.get_u16())
        }
    }

    fn try_get_u32(&mut self) -> Result<u32, TruncatedError> {
        if self.remaining() < 4 {
            Err(TruncatedError)
        } else {
            Ok(self.get_u32())
        }
    }

    fn try_get_i16(&mut self) -> Result<i16, TruncatedError> {
        if self.remaining() < 2 {
            Err(TruncatedError)
        } else {
            Ok(self.get_i16())
        }
    }
}

impl<T: Buf + ?Sized> SafeBuf for T {}

#[derive(Error, Debug)]
pub enum Base128Error {
    #[error("Leading zero in base 128 integer")]
    LeadingZero,
    #[error("More than 5 bytes in base 128 integer")]
    MoreThan5Bytes,
    #[error("Truncated base 128 integer")]
    Truncated,
    #[error("Overflow in base 128 integer")]
    Overflow,
}

impl From<TruncatedError> for Base128Error {
    fn from(_: TruncatedError) -> Base128Error {
        Base128Error::Truncated
    }
}

pub trait BufExt {
    fn get_four_cc(&mut self) -> FourCC;
    fn try_get_four_cc(&mut self) -> Result<FourCC, TruncatedError>;
    fn try_get_base_128(&mut self) -> Result<u32, Base128Error>;
    fn try_get_255_u16(&mut self) -> Result<u16, TruncatedError>;
    fn try_copy_to_buf<T: BufMut>(
        &mut self,
        dest: &mut T,
        num_bytes: usize,
    ) -> Result<(), TruncatedError>;
}

impl<B> BufExt for B
where
    B: Buf,
{
    fn get_four_cc(&mut self) -> FourCC {
        let mut dest = [0; 4];
        self.copy_to_slice(&mut dest);
        FourCC(dest)
    }

    fn try_get_four_cc(&mut self) -> Result<FourCC, TruncatedError> {
        if self.remaining() < 4 {
            Err(TruncatedError)
        } else {
            Ok(self.get_four_cc())
        }
    }

    fn try_get_base_128(&mut self) -> Result<u32, Base128Error> {
        let mut accum = 0u32;
        for i in 0..5 {
            let byte = SafeBuf::try_get_u8(self)?;
            if i == 0 && byte == 0x80 {
                return Err(Base128Error::LeadingZero);
            }
            if accum >> 25 != 0 {
                return Err(Base128Error::Overflow);
            }
            accum = (accum << 7) | ((byte & 0x7F) as u32);
            if byte & 0x80 == 0 {
                return Ok(accum);
            }
        }
        Err(Base128Error::MoreThan5Bytes)
    }

    fn try_get_255_u16(&mut self) -> Result<u16, TruncatedError> {
        const ONE_MORE_BYTE_CODE_1: u8 = 255;
        const ONE_MORE_BYTE_CODE_2: u8 = 254;
        const WORD_CODE: u8 = 253;
        const LOWEST_UCODE: u16 = 253;
        let code = SafeBuf::try_get_u8(self)?;
        match code {
            WORD_CODE => SafeBuf::try_get_u16(self),
            ONE_MORE_BYTE_CODE_1 => Ok(SafeBuf::try_get_u8(self)? as u16 + LOWEST_UCODE),
            ONE_MORE_BYTE_CODE_2 => Ok(SafeBuf::try_get_u8(self)? as u16 + 2 * LOWEST_UCODE),
            _ => Ok(code as u16),
        }
    }

    fn try_copy_to_buf<T: BufMut>(
        &mut self,
        dest: &mut T,
        mut num_bytes: usize,
    ) -> Result<(), TruncatedError> {
        if self.remaining() < num_bytes {
            return Err(TruncatedError);
        }
        loop {
            let chunk = self.chunk();
            if chunk.len() >= num_bytes {
                dest.put_slice(&chunk[..num_bytes]);
                self.advance(num_bytes);
                return Ok(());
            }
            let len = chunk.len();
            dest.put_slice(chunk);
            self.advance(len);
            num_bytes -= len;
        }
    }
}

pub fn pad_to_multiple_of_four(buffer: &mut Vec<u8>) {
    if buffer.len() & 3 != 0 {
        let new_len = (buffer.len() + 3) & !3;
        buffer.resize(new_len, 0);
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use bytes::{Buf, BufMut};

    use super::BufExt;

    fn test_get_255_u16(expected: u16, data: &[u8]) {
        let mut buf = data;
        let result = buf.try_get_255_u16();
        assert_eq!(expected, result.unwrap());
    }

    #[test]
    fn u255_uint_16_0_works() {
        test_get_255_u16(0, &[0u8]);
    }

    #[test]
    fn u255_uint_16_128_works() {
        test_get_255_u16(128, &[128u8]);
    }

    #[test]
    fn u255_uint_16_506_works() {
        test_get_255_u16(506, &[255, 253]);
        test_get_255_u16(506, &[254, 0]);
        test_get_255_u16(506, &[253, 1, 250]);
    }

    #[test]
    fn uint_base_128_0_works() {
        let mut buf = &[0][..];
        let result = buf.try_get_base_128();
        assert_eq!(0, result.unwrap());
    }

    #[test]
    fn uint_base_128_128_works() {
        let mut buf = &[0x81u8, 0u8][..];
        let result = buf.try_get_base_128();
        assert_eq!(128, result.unwrap());
    }

    #[test]
    fn try_copy_to_buf() {
        let mut src: &[u8] = &[42; 11];
        let mut dest = Vec::new();

        src.try_copy_to_buf(&mut dest, 5).unwrap();
        assert_eq!(src.remaining(), 6);
        dest.put_u8(0);
        src.try_copy_to_buf(&mut dest, 5).unwrap();
        assert_eq!(src.remaining(), 1);

        assert!(src.try_copy_to_buf(&mut dest, 2).is_err());

        assert_eq!(dest, &[42, 42, 42, 42, 42, 0, 42, 42, 42, 42, 42]);
    }
}
