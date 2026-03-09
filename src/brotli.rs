use alloc::{boxed::Box, vec, vec::Vec};
use brotli::{Allocator, CustomRead, CustomWrite};
use brotli::{SliceWrapper, SliceWrapperMut};
use bytes::Buf;

struct BrotliBufReader<'a, B>(&'a mut B);

impl<B: Buf> CustomRead<&'static str> for BrotliBufReader<'_, B> {
    fn read(&mut self, data: &mut [u8]) -> Result<usize, &'static str> {
        let len = core::cmp::min(self.0.remaining(), data.len());
        self.0.copy_to_slice(&mut data[..len]);
        Ok(len)
    }
}

struct VecWriter<'a>(&'a mut Vec<u8>);

impl CustomWrite<&'static str> for VecWriter<'_> {
    fn write(&mut self, data: &[u8]) -> Result<usize, &'static str> {
        self.0.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> Result<(), &'static str> {
        Ok(())
    }
}

struct Rebox<T> {
    inner: Box<[T]>,
}

impl<T> Default for Rebox<T> {
    fn default() -> Self {
        Self {
            inner: Vec::new().into_boxed_slice(),
        }
    }
}

impl<T> SliceWrapper<T> for Rebox<T> {
    fn slice(&self) -> &[T] {
        &self.inner
    }
}

impl<T> SliceWrapperMut<T> for Rebox<T> {
    fn slice_mut(&mut self) -> &mut [T] {
        &mut self.inner
    }
}

#[derive(Clone, Copy, Default)]
struct HeapAllocator;

impl<T: Clone + Default> Allocator<T> for HeapAllocator {
    type AllocatedMemory = Rebox<T>;

    fn alloc_cell(&mut self, len: usize) -> Self::AllocatedMemory {
        Rebox {
            inner: vec![T::default(); len].into_boxed_slice(),
        }
    }

    fn free_cell(&mut self, _data: Self::AllocatedMemory) {}
}

pub(crate) fn decompress_to_vec(
    input_buffer: &mut impl Buf,
    output: &mut Vec<u8>,
) -> Result<(), &'static str> {
    let mut input = BrotliBufReader(input_buffer);
    let mut writer = VecWriter(output);
    let mut alloc_u8 = HeapAllocator;
    let mut input_scratch = alloc_u8.alloc_cell(4096);
    let mut output_scratch = alloc_u8.alloc_cell(4096);

    brotli::BrotliDecompressCustomIo(
        &mut input,
        &mut writer,
        input_scratch.slice_mut(),
        output_scratch.slice_mut(),
        alloc_u8,
        HeapAllocator,
        HeapAllocator,
        "Unexpected EOF",
    )?;

    Ok(())
}
