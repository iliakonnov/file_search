use std::io::{Read, Result};

pub struct OffsetedReader<T: Read> {
    reader: T,
    offset: usize,
}

impl<T: Read> OffsetedReader<T> {
    pub fn new(reader: T) -> Self {
        Self { reader, offset: 0 }
    }

    pub fn after(offset: usize, reader: T) -> Self {
        Self { reader, offset }
    }

    pub fn get_offset(&self) -> usize {
        self.offset
    }
}

impl<T: Read> Read for OffsetedReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let res = self.reader.read(buf)?;
        self.offset += res;
        Ok(res)
    }
}
