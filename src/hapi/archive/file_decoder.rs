use crate::hapi::*;

use std::error::Error;
use std::io::{self, prelude::*};

use libflate::zlib;

const HAPI_LZ77_WINDOW_SIZE: usize = 4095; // 2^12 - 1

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
		let mut window = Vec::<u8>::with_capacity(HAPI_LZ77_WINDOW_SIZE as usize + 1);
		window.resize_with(HAPI_LZ77_WINDOW_SIZE as usize + 1, Default::default);
		let mut window_iter = (0..window.len()).peekable();

		let mut input = input.bytes();
		loop {
			if let Some(tag) = input.next() {
				// TODO detect infinite loop
				let tag = tag?;
				for bit in 0..=7 {
					if window_iter.len() == 0 {
						// flush contents
						buffer.extend_from_slice(&window);
						window_iter = (0..window.len()).peekable();
					}

					if tag & (1 << bit) == 0 {
						match input.next() {
							Some(Err(e)) => return Err(e),
							Some(Ok(lit)) => window[window_iter.next().unwrap()] = lit,
							None => return decoder_unexpected_eof,
						}
					} else {
						let lo = input.next().unwrap()? as u16;
						let hi = input.next().unwrap()? as u16;
						let offset = (((hi << 8) | lo) >> 4) as usize;
						if offset != 0 {
							let offset = offset - 1; // now it's an array index
							let count = ((lo & 0x0f) + 2) as usize;
							if offset + count > HAPI_LZ77_WINDOW_SIZE {
								if (offset..offset + count).contains(window_iter.peek().unwrap()) {
									eprintln!(
										"offset {} count {} pos {}",
										offset,
										count,
										window_iter.peek().unwrap()
									);
									return Err(io::Error::new(
										io::ErrorKind::InvalidData,
										"LZ77 pointer overlaps with sliding window position??",
									));
								} else {
									// wow I sure hope count > window_iter.len() is never actually valid
									let window_len = window.len();
									let after_wrap = offset + count & HAPI_LZ77_WINDOW_SIZE;
									let before_wrap = count - after_wrap;
									let dest = *window_iter.peek().unwrap();
									window.copy_within(offset..window_len, dest);
									window.copy_within(0..after_wrap, dest + before_wrap);
									// advance_by isn't stable so Oh Well
									let _ = window_iter.nth(count - 1);
								}
							} else if count > window_iter.len() {
								// flush unwritten window data
								let data_len = *window_iter.peek().unwrap();
								let remaining_len = window_iter.len();
								buffer.extend_from_slice(&window[..data_len]);
								// write from pointed-to data
								window.copy_within(offset..offset + remaining_len, data_len);
								window.copy_within(offset + remaining_len..offset + count, 0);
								// grab what we filled in at the end of the window
								buffer.extend_from_slice(&window[data_len..]);
								// reset window iterator
								window_iter = (data_len + count & HAPI_LZ77_WINDOW_SIZE
									..window.len())
									.peekable();
							} else {
								let dest = *window_iter.peek().unwrap();
								window.copy_within(offset..offset + count, dest);
								// advance_by isn't stable so Oh Well
								let _ = window_iter.nth(count - 1);
							}
						} else {
							// flush unwritten window data to buffer, write all and done
							let data_len = (HAPI_LZ77_WINDOW_SIZE as usize + 1) - window_iter.len();
							buffer.extend_from_slice(&window[..data_len]);
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
