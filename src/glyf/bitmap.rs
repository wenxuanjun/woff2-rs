#[derive(Clone, Copy)]
pub(super) struct Bitmap<'a> {
    bytes: &'a [u8],
}

impl<'a> Bitmap<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub(super) fn is_set(self, index: u16) -> bool {
        let index = index as usize;
        let byte = self.bytes[index / 8];
        let bit = 0x80 >> (index % 8);
        byte & bit != 0
    }
}
