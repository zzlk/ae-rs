use crate::bitio::{BitReader, BitWriter, ReadResult};
use anyhow::Result;
use core::fmt;
use std::io::{Read, Write};

const MAX_SYMBOLS: usize = 0x101;
const MAX_PROBABILITY: usize = 0xFFFFFFFF;
const SYMBOL_EOF: usize = 0x100;

#[derive(Copy, Clone, Debug)]
struct Symbol {
    cumulative_count: usize,
    count: usize,
}

#[derive(Debug)]
struct SymbolTable {
    symbol_count: usize,
    table: [Symbol; MAX_SYMBOLS],
}

impl SymbolTable {
    fn new() -> SymbolTable {
        let mut ret = SymbolTable {
            symbol_count: 0,
            table: [Symbol {
                cumulative_count: 0,
                count: 0,
            }; MAX_SYMBOLS],
        };

        for i in 0..MAX_SYMBOLS {
            ret.increment_symbol(i)
        }

        ret
    }

    fn increment_symbol(&mut self, symbol: usize) {
        self.table[symbol].count += 1;
        self.symbol_count += 1;

        for i in symbol..MAX_SYMBOLS - 1 {
            self.table[i + 1].cumulative_count =
                self.table[i].count + self.table[i].cumulative_count;
        }
    }
}

#[derive(Debug)]
pub struct Encoder<T: Write> {
    high: u32,
    low: u32,
    underflow: usize,

    symbols: SymbolTable,
    bit_writer: BitWriter<T>,
}

#[derive(Debug)]
pub struct Decoder<T: Read> {
    high: u32,
    low: u32,
    code: u32,

    symbols: SymbolTable,
    bit_reader: BitReader<T>,
}

//-------------------------------------------------------------------------------------------------
// calculate_symbol_range
//	Calculates the high and low variables (global variables!!!) for use in encoding.
//-------------------------------------------------------------------------------------------------
impl<T: Write + fmt::Debug> Encoder<T> {
    pub fn new(writer: T) -> Encoder<T> {
        Encoder {
            high: MAX_PROBABILITY as u32,
            low: 0,
            underflow: 0,
            symbols: SymbolTable::new(),
            bit_writer: BitWriter::new(writer),
        }
    }

    fn calculate_symbol_range(&mut self, symbol: usize) {
        assert!(self.high > self.low);

        let range = (self.high - self.low) as usize + 1; // Range is just the difference between the two.

        let symbol_lower = self.symbols.table[symbol].cumulative_count;
        let symbol_upper =
            self.symbols.table[symbol].cumulative_count + self.symbols.table[symbol].count;

        // Now, based on this calculated range, we work out our new range within this range.
        // Think of it as a sort of rescaling operation.
        // We're also subtracting 1 from High because an infinite amount of 0xFFFF follows it, supposedly.
        self.high =
            (self.low as usize + ((symbol_upper * range) / self.symbols.symbol_count) - 1) as u32;
        self.low =
            (self.low as usize + ((symbol_lower * range) / self.symbols.symbol_count)) as u32;

        assert!(self.high > self.low);
    }

    pub fn encode_next(&mut self, symbol: usize) -> Result<()> {
        println!(
            "low: {:08x}, high: {:08x}, underflow: {:08x}",
            self.low, self.high, self.underflow
        );

        // This function will calculate the variables 'high' and 'low' based on our symbol.
        self.calculate_symbol_range(symbol);

        // As high and low converge we want to write out their MSBs.
        // more than 1 of the high bits of low and high can match, but we write out each one per bit.
        loop {
            if (self.high & 0x80000000) == (self.low & 0x80000000) {
                self.bit_writer.write(self.low & 0x80000000 == 0x80000000)?;

                // We need to deal with undeflowing bits here, to make sure that our MSB is shifted to the correct position.
                while self.underflow != 0 {
                    self.bit_writer
                        .write((self.low & 0x80000000) != 0x80000000)?;
                    self.underflow -= 1;
                }
            } else if (self.high & 0xC0000000) == 0x80000000
                && (self.low & 0x40000000) == 0x40000000
            {
                // We've run out of precision, begin implementing hacks.
                self.underflow += 1; // Must keep track of how many bits we obliterate.
                self.low &= 0x3FFFFFFF;
                self.high |= 0x40000000;
            } else {
                break;
            }

            assert!(self.high > self.low);

            // Now that the MSB is gone, we shift it out of high and low.
            self.high = self.high << 1;
            self.low = self.low << 1;

            assert!(self.high > self.low);

            // Remember when we said that High has an infinite amount of 0xFFF following it?
            self.high = self.high | 1;

            assert!(self.high > self.low);

            // However, the next shifted in MSBs might also match, hence the loop.
        }

        // We want to update our probability model now.
        self.symbols.increment_symbol(symbol);

        anyhow::Ok(())
    }

    pub fn encode_end(&mut self) -> Result<()> {
        // Make sure we write an EOF in the stream otherwise disaster.
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

impl<T: Read + fmt::Debug> Decoder<T> {
    pub fn new(reader: T) -> Result<Decoder<T>> {
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

    fn calculate_code_range(&mut self) -> usize {
        assert!(self.high > self.low);

        let range = (self.high - self.low) as usize + 1; // Range is just the difference between the two.

        // let temp = ((self.code - self.low + 1) as usize * MAX_PROBABILITY - 1) / range as usize;
        // println!("temp: {}", temp);
        // let temp = ((temp + 1) * self.symbols.symbol_count) / MAX_PROBABILITY;

        let temp =
            ((self.code as usize - self.low as usize + 1) as usize * self.symbols.symbol_count - 1)
                / range as usize;
        //let temp = ((temp + 1) * self.symbols.symbol_count) / MAX_PROBABILITY;
        println!(
            "temp: {:08x}, self.code: {:08x}, self.low: {:08x}, self.symbol_count: {:08x}, range: {range:08x}",
            temp, self.code, self.low, self.symbols.symbol_count
        );

        // Convert from cumulative count value to a symbol value.
        let mut symbol = MAX_SYMBOLS - 1;
        while self.symbols.table[symbol].cumulative_count > temp {
            symbol -= 1;
        }

        let symbol_lower = self.symbols.table[symbol].cumulative_count;
        let symbol_upper =
            self.symbols.table[symbol].cumulative_count + self.symbols.table[symbol].count;

        // Now, based on this calculated range, we work out our new range within this range.
        // Think of it as a sort of rescaling operation.
        // We're also subtracting 1 from High because an infinite amount of 0xFFFF follows it.
        self.high =
            (self.low as usize + ((symbol_upper * range) / self.symbols.symbol_count) - 1) as u32;
        self.low =
            (self.low as usize + ((symbol_lower * range) / self.symbols.symbol_count)) as u32;

        assert!(self.high > self.low);

        symbol
    }

    pub fn decode_next(&mut self) -> Result<usize> {
        println!(
            "low: {:08x}, high: {:08x}, code: {:08x}-{:032b}",
            self.low, self.high, self.code, self.code
        );
        // This function sets the values high and low!
        let symbol = self.calculate_code_range();

        loop {
            if (self.high & 0x80000000) == (self.low & 0x80000000) {
                // We are basically duplicating our steps from the encode method.
                // We have nothing to encode (we're decoding), so we do nothing, but this still needs to be here.
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

            // Remember when we said that High has an infinite amount of 0xFFF following it?
            self.high = self.high | 1; // There it is.

            // When we were encoding, we had to narrow in on the range with lots of 0xFFFFs, but now, we can get the actual range,
            // We don't want to read garbage data from off the end of the input buffer either.
            match self.bit_reader.read()? {
                crate::bitio::ReadResult::Bit(r) => {
                    self.code |= if r { 1 } else { 0 };
                }
                crate::bitio::ReadResult::EOF => {
                    self.code |= 1;
                }
            }
            assert!(self.high > self.low);

            // However, the next shifted in MSBs might also match, hence the loop.
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
    use crate::ae::SYMBOL_EOF;
    use rand::Rng;
    use rand::SeedableRng;

    #[test]
    fn single_symbol_streams() {
        const COUNT: usize = 4;
        let mut output = [0u8; 8192];

        println!("\n\n\nencoding\n\n");
        for symbol in 0..256 {
            {
                let mut encoder = Encoder::new(output.as_mut_slice());

                for _ in 0..COUNT {
                    encoder.encode_next(symbol).unwrap();
                }
                encoder.encode_end().unwrap();
            }
            //assert_eq!(output, [130, 194, 0, 0]);

            println!("\n\n");
            println!(
                "{:02x}{:02x}{:02x}{:02x}{:02x}",
                output[0], output[1], output[2], output[3], output[4]
            );

            println!("\n\n\ndecoding\n\n");
            {
                let mut decoder = Decoder::new(output.as_slice()).unwrap();

                for _ in 0..COUNT {
                    assert_eq!(decoder.decode_next().unwrap(), symbol);
                }
            }
        }
    }

    #[test]
    fn combinatorial() {
        let mut output = [0u8; 8192];

        println!("\n\n\nencoding\n\n");
        for s3 in 0..256 {
            for s2 in 0..256 {
                for s1 in 0..256 {
                    {
                        let mut encoder = Encoder::new(output.as_mut_slice());
                        encoder.encode_next(s1).unwrap();
                        encoder.encode_next(s2).unwrap();
                        encoder.encode_next(s3).unwrap();
                        encoder.encode_end().unwrap();
                    }
                    //assert_eq!(output, [130, 194, 0, 0]);

                    println!("\n\n");
                    println!(
                        "{:02x}{:02x}{:02x}{:02x}{:02x}",
                        output[0], output[1], output[2], output[3], output[4]
                    );

                    println!("\n\n\ndecoding\n\n");
                    {
                        let mut decoder = Decoder::new(output.as_slice()).unwrap();
                        assert_eq!(decoder.decode_next().unwrap(), s1);
                        assert_eq!(decoder.decode_next().unwrap(), s2);
                        assert_eq!(decoder.decode_next().unwrap(), s3);
                        assert_eq!(decoder.decode_next().unwrap(), SYMBOL_EOF);
                    }
                }
            }
        }
    }

    #[test]
    fn rando() {
        const COUNT: usize = 80000;
        let mut output = [0u8; 204800];

        {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            let mut encoder = Encoder::new(output.as_mut_slice());

            for i in 0..COUNT {
                encoder.encode_next(rng.gen::<usize>() % 256).unwrap();
            }
            encoder.encode_end().unwrap();
        }

        println!("\n\n");
        println!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}",
            output[0], output[1], output[2], output[3], output[4]
        );
        println!("\n\n");

        {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            let mut decoder = Decoder::new(output.as_slice()).unwrap();

            for i in 0..COUNT {
                assert_eq!(decoder.decode_next().unwrap(), rng.gen::<usize>() % 256);
            }
            assert_eq!(decoder.decode_next().unwrap(), SYMBOL_EOF);
        }
    }
}
