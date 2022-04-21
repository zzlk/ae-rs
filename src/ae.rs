use crate::bitio::{BitReader, BitWriter, ReadResult};
use anyhow::Result;
use core::fmt;
use std::io::{Read, Write};

const MAX_SYMBOLS: usize = 0x101;
const MAX_PROBABILITY: usize = 0xFFFFFFFF;
const SYMBOL_EOF: usize = 0x100;

#[derive(Debug)]
struct SymbolTable {
    symbol_count: usize,
    table: [usize; MAX_SYMBOLS + 1],
}

impl SymbolTable {
    fn new() -> SymbolTable {
        let mut ret = SymbolTable {
            symbol_count: 0,
            table: [0; MAX_SYMBOLS + 1],
        };

        for i in 0..MAX_SYMBOLS {
            ret.increment_symbol(i)
        }

        ret
    }

    fn increment_symbol(&mut self, symbol: usize) {
        self.symbol_count += 1;

        for i in symbol..MAX_SYMBOLS {
            self.table[i + 1] += 1;
        }
    }

    fn get_symbol(&self, symbol: usize) -> (usize, usize) {
        (self.table[symbol], self.table[symbol + 1])
    }

    fn find_symbol(&self, cumulative_value: usize) -> (usize, usize, usize) {
        let mut symbol = MAX_SYMBOLS - 1;
        while self.table[symbol] > cumulative_value {
            symbol -= 1;
        }

        (symbol, self.table[symbol], self.table[symbol + 1])
    }
}

#[derive(Debug)]
pub struct Encoder<'a, T: Write> {
    high: u32,
    low: u32,
    underflow: usize,

    symbols: SymbolTable,
    bit_writer: BitWriter<'a, T>,
}

#[derive(Debug)]
pub struct Decoder<'a, T: Read> {
    high: u32,
    low: u32,
    code: u32,

    symbols: SymbolTable,
    bit_reader: BitReader<'a, T>,
}

impl<T: Write + fmt::Debug> Encoder<'_, T> {
    pub fn new(writer: &mut T) -> Encoder<'_, T> {
        Encoder {
            high: MAX_PROBABILITY as u32,
            low: 0,
            underflow: 0,
            symbols: SymbolTable::new(),
            bit_writer: BitWriter::new(writer),
        }
    }

    pub fn encode_next(&mut self, symbol: usize) -> Result<()> {
        let range = (self.high - self.low) as usize + 1;

        // should probably make this a part of the model.
        let (symbol_low, symbol_high) = self.symbols.get_symbol(symbol);

        // rescale low and high so that the new low and high are proportional to the cumulative frequency in the model.
        // for example if low = 0, high = 1, there's 2 symbols (A, B) with probability 1/3 and 2/3, then if we encode an A the
        // next [low, high) should be [0, 1/3). If we encode a B then [low, high] should be [1/3rd, 1),
        // except all of this is with integers, so there's +1 and -1 in various places to prevent truncation issues.
        self.high =
            (self.low as usize + ((symbol_high * range) / self.symbols.symbol_count) - 1) as u32;
        self.low = (self.low as usize + ((symbol_low * range) / self.symbols.symbol_count)) as u32;

        // As high and low converge we want to write out their MSBs.
        loop {
            if (self.high & 0x80000000) == (self.low & 0x80000000) {
                self.bit_writer.write(self.low & 0x80000000 == 0x80000000)?;

                // When we run out of precision, we remember how many bits are obliterated so that we don't run out of precision.
                // Once we discover the true MSB then we can output that number of bits correctly.
                while self.underflow != 0 {
                    self.bit_writer
                        .write((self.low & 0x80000000) != 0x80000000)?;
                    self.underflow -= 1;
                }
            } else if (self.high & 0xC0000000) == 0x80000000
                && (self.low & 0x40000000) == 0x40000000
            {
                // We've run out of precision, begin implementing hacks.
                // this is probably one of the trickiest parts.
                // if low is converging on 0x7FFFFFF... and high is converging on 0x80000....
                // then we don't know what bit to output because the MSB of low and high do not match yet.
                // but if we keep going then we will run out of integer precision before the msb matches.
                // so, we basically shift everything left and keep a counter of how many times we have done that.
                // eventually either low will go above 0x7fff...  or high will go below 0x8000.... at that point we can output
                // the MSB followed by <underflow> opposite bits
                self.underflow += 1; // Must keep track of how many bits we obliterate.
                self.low &= 0x3FFFFFFF;
                self.high |= 0x40000000;
            } else {
                break;
            }

            // Now that the MSB is gone, we shift it out of high and low.
            self.high = self.high << 1;
            self.low = self.low << 1;

            // conceptually high has an infinite stream of 1 bits following it, and low has an infinite stream of 0 bits following it.
            self.high = self.high | 1;

            // The next shifted in MSBs might also match, so we loop.
        }

        self.symbols.increment_symbol(symbol);

        anyhow::Ok(())
    }

    pub fn encode_end(&mut self) -> Result<()> {
        self.encode_next(SYMBOL_EOF)?;

        self.underflow += 1;
        self.bit_writer.write(self.low & 0x40000000 == 0x40000000)?;

        while self.underflow > 0 {
            self.underflow -= 1;
            self.bit_writer.write(self.low & 0x40000000 != 0x40000000)?;
        }

        self.bit_writer.flush()?;

        anyhow::Ok(())
    }
}

impl<T: Read + fmt::Debug> Decoder<'_, T> {
    pub fn new(reader: &mut T) -> Result<Decoder<'_, T>> {
        let mut decoder = Decoder {
            high: 0xFFFFFFFF,
            low: 0,
            symbols: SymbolTable::new(),
            bit_reader: BitReader::new(reader),
            code: 0,
        };

        for _ in 0..32 {
            decoder.code = decoder.code << 1;
            match decoder.bit_reader.read()? {
                ReadResult::EOF => decoder.code = decoder.code | 1,
                ReadResult::Bit(r) => decoder.code = decoder.code | if r { 1 } else { 0 },
            }
        }

        anyhow::Ok(decoder)
    }

    pub fn decode_next(&mut self) -> Result<usize> {
        // Decoding is almost identical to encoding except that we have a stream of already encoded bits that we have to deal with.
        let range = (self.high - self.low) as usize + 1;

        // This is essentially the major difference between encoding and decoding.
        // In decoding we determine the symbol from the already encoded stream by where it lies in the range between high and low.
        // in encoding we calculate the range directly as we are given the symbol.
        let cumulative_value =
            ((self.code as usize - self.low as usize + 1) as usize * self.symbols.symbol_count - 1)
                / range as usize;

        let (symbol, symbol_low, symbol_high) = self.symbols.find_symbol(cumulative_value);

        // The following is identical to encoding.
        self.high =
            (self.low as usize + ((symbol_high * range) / self.symbols.symbol_count) - 1) as u32;
        self.low = (self.low as usize + ((symbol_low * range) / self.symbols.symbol_count)) as u32;

        loop {
            if (self.high & 0x80000000) == (self.low & 0x80000000) {
                // Since we are decoding then there's nothing to do here.
                // We need to preserve the condition because the second branch in this if statement has the assumption that the above is not true.
            } else if (self.high & 0xC0000000) == 0x80000000
                && (self.low & 0x40000000) == 0x40000000
            {
                // More precision hacks.
                self.high = self.high | 0x40000000;
                self.low = self.low & 0x3FFFFFFF;

                self.code -= 0x40000000;
            } else {
                // Can't do anything.
                break;
            }

            // Now that the MSB is gone, we shift it out of high and low.
            self.high = self.high << 1;
            self.low = self.low << 1;
            self.code = self.code << 1;

            self.high = self.high | 1; // There it is.

            // This is the other major difference from encoding.
            // This is just reading the stream of bits from the encoded value, we don't have this while encoding.
            match self.bit_reader.read()? {
                crate::bitio::ReadResult::Bit(r) => {
                    self.code |= if r { 1 } else { 0 };
                }
                crate::bitio::ReadResult::EOF => {
                    // I think that when decoding a well-formed stream, the actual decoder never processes these bits
                    // but for extremely small messages the decoder starts with 4 bytes of read data so this can actually be invoked.
                    self.code |= 1;
                }
            }

            // The next shifted in MSBs might also match, so we loop.
        }

        // We want to update our probability model now.
        self.symbols.increment_symbol(symbol);

        anyhow::Ok(symbol)
    }
}

#[cfg(test)]
mod test {
    use super::Decoder;
    use super::Encoder;
    use super::SYMBOL_EOF;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn can_read_and_write_same_bytes(input: Vec<u8>) {
        let mut output = Vec::new();
        let mut output2 = Vec::new();

        {
            let mut encoder = Encoder::new(&mut output);
            for s in &input {
                encoder.encode_next(*s as usize).unwrap();
            }
            encoder.encode_end().unwrap();
        }

        {
            let mut cursor = std::io::Cursor::new(&output);
            let mut decoder = Decoder::new(&mut cursor).unwrap();

            loop {
                let s = decoder.decode_next().unwrap();

                if s == SYMBOL_EOF {
                    break;
                }

                output2.push(s as u8);
            }
        }

        assert_eq!(input, output2);
    }
}
