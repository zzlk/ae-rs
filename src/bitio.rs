use anyhow::Result;
use std::io::{Read, Write};

#[derive(Debug)]
pub(crate) struct BitWriter<Writer: Write> {
    writer: Writer,
    buffer_length: usize,
    buffer: u8,
}

#[derive(Debug)]
pub(crate) struct BitReader<Reader: Read> {
    reader: Reader,
    buffer_length: usize,
    buffer: u8,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ReadResult {
    EOF,
    Bit(bool),
}

impl<T: Read> BitReader<T> {
    pub(crate) fn new(reader: T) -> BitReader<T> {
        BitReader {
            reader,
            buffer_length: 0,
            buffer: 0,
        }
    }

    pub(crate) fn read(&mut self) -> Result<ReadResult> {
        if self.buffer_length == 0 {
            let mut buff: &mut [u8] = &mut [0];

            if self.reader.read(&mut buff)? == 1 {
                self.buffer = buff[0];
                self.buffer_length = 8;
            } else {
                return Ok(ReadResult::EOF);
            }
        }
        let ret = self.buffer & (1 << (self.buffer_length - 1)) != 0;
        self.buffer_length -= 1;
        Ok(ReadResult::Bit(ret))
    }
}

impl<T: Write> BitWriter<T> {
    pub(crate) fn new(writer: T) -> BitWriter<T> {
        BitWriter {
            writer,
            buffer_length: 0,
            buffer: 0,
        }
    }

    pub(crate) fn write(&mut self, x: bool) -> Result<()> {
        self.buffer |= (if x { 1 } else { 0 }) << (7 - self.buffer_length);
        self.buffer_length += 1;

        if self.buffer_length == 8 {
            anyhow::ensure!(self.writer.write(&[self.buffer])? == 1);
            self.buffer_length = 0;
            self.buffer = 0;
        }

        Ok(())
    }

    pub(crate) fn flush(&mut self) -> Result<()> {
        if self.buffer_length > 0 {
            anyhow::ensure!(self.writer.write(&[self.buffer])? == 1);
            self.buffer_length = 0;
            self.buffer = 0;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::ReadResult;
    use super::{BitReader, BitWriter};
    use rand::{Rng, SeedableRng};

    #[test]
    fn test_reader() {
        let src: &[u8] = &[0b11110000, 0b00001111];
        let mut reader = BitReader::new(src);

        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));

        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));

        assert_eq!(reader.read().unwrap(), ReadResult::EOF);
        assert_eq!(reader.read().unwrap(), ReadResult::EOF);
    }

    #[test]
    fn test_writer() {
        let mut src = [0u8; 2];

        {
            let mut writer = BitWriter::new(src.as_mut_slice());

            writer.write(true).unwrap();
            writer.write(true).unwrap();
            writer.write(true).unwrap();
            writer.write(true).unwrap();

            writer.write(false).unwrap();
            writer.write(false).unwrap();
            writer.write(false).unwrap();
            writer.write(false).unwrap();

            writer.write(false).unwrap();
            writer.write(false).unwrap();
            writer.write(false).unwrap();
            writer.write(false).unwrap();

            writer.write(true).unwrap();
            writer.write(true).unwrap();
            writer.write(true).unwrap();
            writer.write(true).unwrap();
        }

        assert_eq!(src, [0b11110000, 0b00001111]);
    }

    #[test]
    fn test_random() {
        const LENGTH: usize = 32768;
        let mut output = [0u8; LENGTH];

        {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            let mut writer = BitWriter::new(output.as_mut_slice());

            for _ in 0..LENGTH * 8 {
                writer.write(rng.gen::<bool>()).unwrap();
            }

            writer.flush().unwrap();
        }

        {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            let mut reader = BitReader::new(output.as_slice());

            for _ in 0..LENGTH * 8 {
                assert_eq!(reader.read().unwrap(), ReadResult::Bit(rng.gen::<bool>()));
            }
        }
    }
}
