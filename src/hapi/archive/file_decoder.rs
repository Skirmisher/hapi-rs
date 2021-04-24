use crate::hapi::*;

use std::error::Error;
use std::io::{self, prelude::*};

use libflate::zlib;
use rle_decode_fast::rle_decode;

const HAPI_LZ77_WINDOW_SIZE: u16 = 4095; // 2^12 - 1

#[derive(Debug)]
struct HapiChunkDecoder<'a> {
	source: &'a HapiCompressedChunk,
	cur_pos: usize,
}

impl<'a> HapiChunkDecoder<'a> {
	fn new(source: &'a HapiCompressedChunk) -> Self {
		HapiChunkDecoder { source, cur_pos: 0 }
	}
}

impl Read for HapiChunkDecoder<'_> {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		let bytes_count = (&self.source.data[self.cur_pos..]).read(buf)?;

		if self.source.is_enciphered {
			// we have to fuckin
			for (mut count, byte) in buf.iter_mut().enumerate().take(bytes_count) {
				count += self.cur_pos;

				*byte = ((*byte as usize).wrapping_sub(count) ^ count) as u8;
			}
		}

		self.cur_pos += bytes_count;

		Ok(bytes_count)
	}
}

impl HapiCompressedChunk {
	pub(super) fn decompress<W: Write>(&self, output: &mut W) -> Result<(), Box<dyn Error>> {
		let data = HapiChunkDecoder::new(&self);

		let real_size = match self.compression {
			HapiCompressionType::None => {
				unreachable!("chunk with HapiCompressionType::None passed to decompress()")
			}
			HapiCompressionType::Lz77 => self.decode_lz77(data, output)?,
			HapiCompressionType::Zlib => io::copy(&mut zlib::Decoder::new(data)?, output)?,
		};

		if real_size != self.decompressed_size as u64 {
			eprintln!(
				"Warning: chunk had inaccurate decompressed size (given {}, actual {}). \
						Archive may be corrupt.",
				self.decompressed_size, real_size
			);
		}

		Ok(())
	}

	fn decode_lz77<W: Write>(&self, input: HapiChunkDecoder, output: &mut W) -> io::Result<u64> {
		let decoder_unexpected_eof = Err(io::Error::new(
			io::ErrorKind::UnexpectedEof,
			"LZ77 decoding ended prematurely",
		));

		let mut buffer = Vec::<u8>::with_capacity(self.decompressed_size as usize);

		let mut input = input.bytes();
		loop {
			if let Some(tag) = input.next() {
				// TODO detect infinite loop
				let tag = tag?;
				for bit in 0..=7 {
					if tag & (1 << bit) == 0 {
						match input.next() {
							Some(Err(e)) => return Err(e),
							Some(Ok(lit)) => buffer.push(lit),
							None => return decoder_unexpected_eof,
						}
					} else {
						let lo = input.next().unwrap()? as u16;
						let hi = input.next().unwrap()? as u16;
						let offset = ((hi << 8) | lo) >> 4;
						if offset > HAPI_LZ77_WINDOW_SIZE {
							return Err(io::Error::new(
								io::ErrorKind::InvalidData,
								"LZ77 pointer longer than history buffer",
							));
						} else if offset != 0 {
							let count = (lo & 0x0f) + 2;
							let start = buffer.len().saturating_sub(HAPI_LZ77_WINDOW_SIZE as usize);
							let r_offset = buffer.len() - (start + offset as usize) + 1;
							rle_decode(&mut buffer, r_offset, count as usize);
						} else {
							output.write_all(&buffer)?;
							return Ok(buffer.len() as u64);
						}
					}
				}
			} else {
				return decoder_unexpected_eof;
			}
		}
	}
}
