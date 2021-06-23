//! Contains structs like the all-important [Archive] struct 

use std::{io::{Read, Seek, SeekFrom, Write}, marker::PhantomData, u8};
use thiserror::Error;

use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Numeric};
use std::convert::{TryFrom, TryInto};

use super::Optimize;

trait ReadByteExt {
    fn byte(&mut self) -> u8;

    fn bytes_at(&mut self, pos: u64, len: u64) -> Vec<u8>;

    fn byte_at(&mut self, pos: u64) -> u8;
}

impl<R: Read + Seek> ReadByteExt for R {
    /// Read a single byte from the reader and return it
    fn byte(&mut self) -> u8 {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf).unwrap(); //Read the byte 
        buf[0]
    }

    /// Read a certain number of bytes from a position
    fn bytes_at(&mut self, pos: u64, len: u64) -> Vec<u8> {
        let mut out = vec![0u8 ; len as usize];
        self.seek(SeekFrom::Start(pos)).unwrap();
        self.read_exact(&mut out).unwrap(); //Read the bytes into the output
        out
    }
    
    /// Read a single byte at a certain position, and leave the current reader position there
    #[inline]
    fn byte_at(&mut self, pos: u64) -> u8 {
        self.seek(SeekFrom::Start(pos)).unwrap(); //Seek to the position
        self.byte()
    }

}

/// The `Lz77` struct compresses any type that implements the `Read` and `Seek` traits using the lz77 compression
/// algorithm
pub struct Lz77<R: Read + Seek> {
    /// The input buffer that we are compressing
    data: R,
}

impl<R: Read + Seek> Lz77<R> {

    /// Create a new Lz77 compressor from an input reader
    pub fn new(data: R) -> Self {
        Self {
            data
        }
    }

    /// Return the length of the input bytes using seek operations
    fn len(&mut self) -> u64 {  
        self.data.seek(SeekFrom::End(0)).unwrap(); //Seek to the end of the stream
        let len = self.data.stream_position().unwrap(); //Get the current stream position
        len
    }

    /// Compress the input reader into a vector of bytes
    pub fn compress(&mut self) -> Vec<u8> {
        let mut out = Vec::new(); //Create an output buffer

        let mut pos = 0u64;  //Start at byte 0
        let len = self.len();
        while pos < len {
            let (off, matchlen) = self.longest_match(pos); //Get the best match in the previous data
            out.push(off); //Write the 0 offset
            if off == 0 {
                out.push(self.data.byte_at(pos)); //Write the byte literal
                pos += 1;
            }
            else {
                out.push(matchlen);
                pos += matchlen as u64;
            }
        }

        out
    }

    /// Compress data but show it as a human readable format
    pub fn debug_compress(&mut self) -> String {
        let mut out = String::new(); //Create an output buffer

        let mut pos = 0u64;  //Start at byte 0
        let len = self.len();
        while pos < len {
            let (off, matchlen) = self.longest_match(pos); //Get the best match in the previous data
            if off == 0 {
                out.push_str("(0)");
                out.push(self.data.byte_at(pos) as char);
                out.push(' ');
                pos += 1;
            }
            else {
                out.push_str(&*format!("({}, {}):({}) ", off, matchlen, String::from_utf8(self.data.bytes_at(pos - off as u64, matchlen as u64)).unwrap()));
                pos += matchlen as u64;
            }
        }

        out
    }

    /// Decompress the input data, assumes that it is valid Lz77 format
    pub fn decompress(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0u64;
        let len = self.len();
        while pos + 1 < len {
            let (first, second) = (self.data.byte(), self.data.byte());
            pos += 2;
            match (first, second) {
                //Null pointer, byte literal
                (0u8, item) => {
                    out.push(item);
                },
                //Offset and len to read
                (offset, mut match_len) => {
                    let offpos = out.len() - offset as usize;
                    let mut matching = Vec::new();
                    while match_len > 0 { 
                        if match_len >= offset {
                            match_len -= offset;
                            for i in offpos..(offpos + offset as usize) {
                                matching.push(out[i])
                            }
                        } else {
                            for i in offpos..(offpos + match_len as usize) {
                                matching.push(out[i])
                            }
                            break
                        }
                    }
                    out.extend(matching);
                }
            }
        }

        out
    }

    /// Search our window for the longest match and return the pair of (offset, len) or (0, 0) if there is no match
    fn longest_match(&mut self, pos: u64) -> (u8, u8) {
        let mut bestoff = 0u8; //The best offset that we have found
        let mut bestlen = 0u8;
        //Get the start position to seek to 
        let start = if pos > 255 {
            pos - 255
        } else {0};

        for off in start..pos {
            let len = self.match_len(off, pos);
            if len > bestlen {
                bestoff = (pos - off) as u8;
                bestlen = len;
            }
        }

        (bestoff, bestlen)
    }

    /// Return the number of bytes that match between the current offset and the position
    fn match_len(&mut self, mut off: u64, mut pos: u64) -> u8 {
        let old_pos = self.data.stream_position().unwrap(); //Get the current stream position to restore
        let mut len = 0u8; //The length of the matched string

        while off < pos 
            && pos < self.len()
            && self.data.byte_at(off) == self.data.byte_at(pos)
            && len < u8::MAX {
                pos += 1;
                off += 1;
                len += 1;
        }

        self.data.seek(SeekFrom::Start(old_pos)).unwrap(); 
        len
    }
}



/// The `LzSS` struct compresses any type that implements the `Read` and `Seek` traits using the lzss compression
/// algorithm. It has a selectable window size and requires bitwise I/O for compression and decompression,
/// but can often compress more than lz77
pub struct LzSS<R: Read + Seek, Window: Numeric + TryFrom<usize> + std::ops::AddAssign<Window> + TryInto<usize> = u32>
where <Window as TryFrom<usize>>::Error: std::fmt::Display + std::fmt::Debug,
<Window as TryInto<usize>>::Error: std::fmt::Display + std::fmt::Debug {
    /// The input buffer that we are compressing
    data: R,

    /// We need phantom data because the Window type isn't used in any field
    _phantom: PhantomData<Window>,
}

/// Any error that can occur when compressing or decompressing with the [LzSS] algorithm
#[derive(Error, Debug)]
pub enum LzSSErr {
    #[error("An internal Input/Output error occurred")]
    IO(#[from] std::io::Error)
}

type LzSSResult<T> = Result<T, LzSSErr>;

impl<R: Read + Seek, Window: Numeric + TryFrom<usize> + std::ops::AddAssign<Window> + TryInto<usize>> LzSS<R, Window>
where <Window as TryFrom<usize>>::Error: std::fmt::Display + std::fmt::Debug,
<Window as TryInto<usize>>::Error: std::fmt::Display + std::fmt::Debug {
    /// Create a new Lz77 compressor from an input reader
    pub fn new(data: R) -> Self {
        Self {
            data,
            _phantom: PhantomData
        }
    }

    /// Return the length of the input bytes using seek operations
    fn len(&mut self) -> LzSSResult<u64> {  
        self.data.seek(SeekFrom::End(0))?; //Seek to the end of the stream
        let len = self.data.stream_position()?; //Get the current stream position
        Ok(len)
    }

    /// Compress the input reader into a vector of bytes
    pub fn compress<W: Write>(&mut self, writer: &mut W, opt: Optimize) -> LzSSResult<()> {
        let mut out = BitWriter::endian(writer, bitstream_io::LittleEndian); //Create an output buffer

        let mut pos = 0u64;  //Start at byte 0
        let len = self.len()?;
        while pos < len {
            let (off, matchlen) = self.longest_match(pos)?; //Get the best match in the previous data
            if off == 0.try_into().unwrap() {
                out.write_bit(true)?; //Write that this is a literal
                out.write::<u8>(8, self.data.byte_at(pos))?; //Write the byte literal
                pos += 1;
            }
            else {
                out.write_bit(false)?; //Indicate that this is a pointer
                out.write::<Window>(Window::bits_size(), off)?;
                out.write::<Window>(Window::bits_size(), matchlen)?;
                pos += matchlen.try_into().unwrap() as u64;
            }
        }

        out.byte_align()?;
        Ok(())
    }

    /// Compress data but show it as a human readable format
    pub fn debug_compress(&mut self) -> LzSSResult<String> {
        let mut out = String::new(); //Create an output buffer

        let mut pos = 0u64;  //Start at byte 0
        let len = self.len()?;
        while pos < len {
            let (off, matchlen) = self.longest_match(pos)?; //Get the best match in the previous data
            if off == 0.try_into().unwrap() {
                out.push_str("(1)");
                out.push(self.data.byte_at(pos) as char);
                out.push(' ');
                pos += 1;
            }
            else {
                out.push_str(&*format!("(0)({}, {}):({}) ", off.try_into().unwrap(), matchlen.try_into().unwrap(), String::from_utf8(self.data.bytes_at(pos - off.try_into().unwrap() as u64, matchlen.try_into().unwrap() as u64)).unwrap()));
                pos += matchlen.try_into().unwrap() as u64;
            }
        }

        Ok(out)
    }

    /// Return the bitsizes for a given optimization level
    fn opt_bitsize(opt: &Optimize) -> u32 {
        match opt {
            Optimize::Large => 16, //65536B window size for large files
            Optimize::Average => 15, //32768B for average files
            Optimize::Small => 10, //1024B for small files
        }
    }

    /// Get the maximum value for a given optimization level, this is used to determine window size
    #[inline(always)]
    fn opt_max(opt: &Optimize) -> usize {
        2usize.pow(Self::opt_bitsize(opt)) - 1
    }

    /// Decompress the input data, assumes that it is valid Lz77 format
    pub fn decompress<W: Write + Read + Seek>(&mut self, out: &mut W) -> LzSSResult<()> {
        let mut pos = 0u64; 
        let mut out_len = 0usize; //The length of the output in bytes

        let len = self.len()? * 8;
        self.data.seek(SeekFrom::Start(0))?;

        let mut bits = BitReader::endian(&mut self.data, bitstream_io::LittleEndian);
        while pos + 8 < len {
            let sign = bits.read_bit()?; //Read the bit that determines if this is a literal or a pointer
            pos += 1;
            match sign {
                true => {
                    let literal = bits.read::<u8>(8).unwrap(); //Read the raw byte
                    out.write(&[literal]);
                    out_len += 1;
                    pos += 8;
                },
                false => {  
                    let (offset, match_len) = (bits.read::<Window>(Window::bits_size())?, bits.read::<Window>(Window::bits_size())?);
                    let offset = offset.try_into().unwrap();
                    let mut match_len = match_len.try_into().unwrap();
                    pos += (Window::bits_size() * 2) as u64;

                    let offpos = out_len - offset as usize;
                    let mut matching = Vec::new();
                    while match_len > 0 { 
                        if match_len >= offset {
                            match_len -= offset;
                            for i in offpos..(offpos + offset as usize) {
                                matching.push(out.byte_at(i as u64))

                            }
                        } else {
                            for i in offpos..(offpos + match_len as usize) {
                                matching.push(out.byte_at(i as u64))
                            }
                            break
                        }
                    }
                    out.write_all(matching.as_ref());
                    out_len += matching.len();
                }
            }
        }

        Ok(())
    }

    /// Search our window for the longest match and return the pair of (offset, len) or (0, 0) if there is no match
    fn longest_match(&mut self, pos: u64) -> LzSSResult<(Window, Window)> {
        let mut bestoff: Window = Window::try_from(0).unwrap(); //The best offset that we have found
        let mut bestlen: Window = Window::try_from(0).unwrap();
        //Get the start position to seek to 
        let start = if pos > self.max_window_val().try_into().unwrap() as u64 {
            pos - self.max_window_val().try_into().unwrap() as u64
        } else {0};

        for off in start..pos {
            let len = self.match_len(off, pos)?;
            if len > bestlen {
                bestoff = Window::try_from((pos - off) as usize).unwrap();
                bestlen = len;
            }
        }

        //If we don't break even, then return 0
        Ok(if bestlen < Window::try_from((Window::bits_size() / 4 )as usize).unwrap() {
            (Window::try_from(0).unwrap(), Window::try_from(0).unwrap())
        }
        else {(bestoff, bestlen)})

        
    }

    /// Get the maximum value of the `Window` type
    #[inline(always)]
    fn max_window_val(&self) -> Window {
        Window::try_from((2usize.pow(Window::bits_size()) - 1) as usize).unwrap()
    }

    /// Return the number of bytes that match between the current offset and the position
    fn match_len(&mut self, mut off: u64, mut pos: u64) -> LzSSResult<Window> {
        let old_pos = self.data.stream_position().unwrap(); //Get the current stream position to restore
        let mut len = Window::try_from(0).unwrap(); //The length of the matched string

        while off < pos 
            && pos < self.len()?
            && self.data.byte_at(off) == self.data.byte_at(pos)
            && len < self.max_window_val() {
                pos += 1;
                off += 1;
                len += Window::try_from(1).unwrap();
        }

        self.data.seek(SeekFrom::Start(old_pos)).unwrap(); 
        Ok(len)
    }
}

/// The `LzSSCompressor` is a simple facade struct that is used to give an implementation of [Compressor](super::Compressor) to the 
/// [LzSS] struct, using optimization level enums instead of 
pub struct LzSSCompressor();