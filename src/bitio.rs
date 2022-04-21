use anyhow::Result;
use std::io::{Read, Write};

#[derive(Debug)]
pub(crate) struct BitWriter<'a, Writer: Write> {
    writer: &'a mut Writer,
    buffer_length: usize,
    buffer: u8,
}

#[derive(Debug)]
pub(crate) struct BitReader<'a, Reader: Read> {
    reader: &'a mut Reader,
    buffer_length: usize,
    buffer: u8,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ReadResult {
    EOF,
    Bit(bool),
}

impl<T: Read> BitReader<'_, T> {
    pub(crate) fn new(reader: &mut T) -> BitReader<T> {
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

impl<T: Write> BitWriter<'_, T> {
    pub(crate) fn new(writer: &mut T) -> BitWriter<'_, T> {
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
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn can_read_and_write_same_data(input: Vec<u8>) {
        let mut output = Vec::new();

        {
            let mut writer = BitWriter::new(&mut output);
            let mut cursor = std::io::Cursor::new(&input);
            let mut reader = BitReader::new(&mut cursor);

            for _ in 0..input.len() * 8 {
                writer
                    .write(match reader.read().unwrap() {
                        ReadResult::EOF => panic!(),
                        ReadResult::Bit(r) => r,
                    })
                    .unwrap();
            }

            writer.flush().unwrap();
        }

        assert_eq!(input, output);
    }

    #[test]
    fn test_in_memory_representation_reader() {
        // The bits are written MSB first. I'm not sure what the right way is here, either way works.
        let src = vec![0b11110000];
        let mut cursor = std::io::Cursor::new(src);
        let mut reader = BitReader::new(&mut cursor);

        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(true));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));
        assert_eq!(reader.read().unwrap(), ReadResult::Bit(false));

        assert_eq!(reader.read().unwrap(), ReadResult::EOF);
        assert_eq!(reader.read().unwrap(), ReadResult::EOF);
    }

    #[test]
    fn test_writer() {
        // The bits are written MSB first. I'm not sure what the right way is here, either way works.
        let mut output = Vec::new();

        {
            let mut writer = BitWriter::new(&mut output);

            writer.write(true).unwrap();
            writer.write(true).unwrap();
            writer.write(true).unwrap();
            writer.write(true).unwrap();

            writer.write(false).unwrap();
            writer.write(false).unwrap();
            writer.write(false).unwrap();
            writer.write(false).unwrap();
        }

        assert_eq!(output, [0b11110000]);
    }
}
