use std::error::Error;
use std::fs::{File, ReadDir};
use std::io::{self, prelude::*, BufReader};
use std::path::{Path, PathBuf};

use binread::BinRead;
use libflate::zlib;
use rle_decode_fast::rle_decode;

use super::*;
use std::fmt::Debug;

const HAPI_LZ77_WINDOW_SIZE: u16 = 4095; // 2^12 - 1

#[derive(Debug)]
enum OutputTarget<W: Write> {
	Stream(W),
	Directory(PathBuf),
}

#[derive(Debug)]
pub struct HapiArchive<R: Read + Seek> {
	reader: HapiReader<R>,
	contents: HapiDirectory,
}

impl<R> HapiArchive<R>
where
	R: Read + Seek + Debug,
{
	pub fn open(stream: R) -> Result<HapiArchive<R>, Box<dyn Error>> {
		// Create reader
		let reader = BufReader::new(stream);
		let mut reader = HapiReader::new(reader)?;

		// Parse table of contents
		reader.seek(SeekFrom::Start(reader.header.toc_offset as u64))?;
		let contents = HapiDirectory::read(&mut reader)?;

		let mut archive = HapiArchive { reader, contents };

		eprintln!("{:#x?}", archive);

		// FIXME remove extraction test
		archive.extract_file(
			if let HapiEntry::File(file) = &archive.contents.contents[0].entry.clone() {
				&file
			} else {
				panic!()
			},
			&mut std::io::stdout(),
		)?;

		Ok(archive)
	}

	#[cfg(fucko)]
	fn extract_entry<W: Write>(
		&mut self,
		entry: &HapiEntry,
		output: &mut OutputTarget<W>,
	) -> io::Result<()> {
		match entry {
			HapiEntry::File(file) => match output {
				OutputTarget::Stream(output) => self.extract_file(&file, output),
				OutputTarget::Directory(path) => {
					if path.is_dir() {
						let mut output = File::create(path.with_file_name(&file.name))?;
						self.extract_file(&file, &mut output)
					} else {
						Err(io::Error::new(ErrorKind::InvalidInput, "Not a directory"))
					}
				}
			},
			HapiEntry::Directory(directory) => {
				assert!(matches!(output, OutputTarget::Directory(_))); // ok but tar output support when
				for entry in &directory.contents {
					self.extract_entry(&entry, output)?;
				}
				Ok(())
			}
		}
	}

	fn extract_file<W: Write>(
		&mut self,
		entry: &HapiFile,
		output: &mut W,
	) -> Result<(), Box<dyn Error>> {
		self.reader
			.seek(SeekFrom::Start(entry.contents_offset as u64))?;
		let contents = HapiFileContents::read_args(
			&mut self.reader,
			(entry.extracted_size, entry.compression),
		)?;

		match contents {
			HapiFileContents::Uncompressed(data) => Ok(output.write_all(&data)?),
			HapiFileContents::Compressed(chunks, ..) => {
				chunks.iter().try_for_each(|chunk| chunk.decompress(output))
			}
		}
	}
}

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

		//eprintln!("HapiChunkDecoder::read called, read {} bytes", bytes_count);

		if self.source.is_enciphered {
			// we have to fuckin
			for (mut count, byte) in buf.iter_mut().enumerate().take(bytes_count) {
				count += self.cur_pos;

				*byte = ((*byte as usize).wrapping_sub(count) ^ count) as u8;
			}
		}

		self.cur_pos += bytes_count;

		// eprintln!(
		// 	"we read {} data, it looks like [{}], current pos now {}",
		// 	bytes_count,
		// 	String::from_utf8_lossy(buf),
		// 	self.cur_pos
		// );

		Ok(bytes_count)
	}
}

impl HapiCompressedChunk {
	fn decompress<W: Write>(&self, output: &mut W) -> Result<(), Box<dyn Error>> {
		let data = HapiChunkDecoder::new(&self);

		eprintln!("{:#x?}", data);

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
				// eprintln!("starting with tag {:08b}", tag);
				for bit in 0..=7 {
					// eprintln!("we think current tag bit is {}", (tag & (1 << bit) != 0));
					if tag & (1 << bit) == 0 {
						match input.next() {
							Some(Err(e)) => return Err(e),
							Some(Ok(lit)) => {
								// eprintln!("decoding literal {:#x?}", lit);
								buffer.push(lit);
							}
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
							// eprintln!(
							// 	"decoding pointer, offset {:#x?} count {:#x?}",
							// 	offset, count
							// );
							let start = buffer.len().saturating_sub(HAPI_LZ77_WINDOW_SIZE as usize);
							let r_offset = buffer.len() - (start + offset as usize) + 1;
							rle_decode(&mut buffer, r_offset, count as usize);
						} else {
							// eprintln!("done decoding");
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
