use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{self, prelude::*, BufReader, SeekFrom};
use std::path::Path;

const HAPI_MAGIC: &[u8] = b"HAPI";
const HAPI_SAVE_MARKER: &[u8] = b"BANK";
const HAPI_ARCHIVE_MARKER: &[u8] = &[0x00, 0x00, 0x01, 0x00];

struct HapiArchive {
	reader: BufReader<File>,
	key: u32,
	start_offset: u32,
}

// HAPI header structure: 20 bytes
#[derive(Debug)]
struct HapiHeader {
	magic: [u8; 4],    // HAPI_MAGIC
	marker: [u8; 4],   // HAPI_SAVE_MARKER or HAPI_ARCHIVE_MARKER
	size: u32,         // size of table of contents
	key: u32,          // XOR cipher key
	start_offset: u32, // offset of contents from file start
}

impl HapiArchive {
	fn parse_header(&mut self) -> io::Result<()> {
		let mut header = HapiHeader {
			magic: [0u8; 4],
			marker: [0u8; 4],
			size: 0u32,
			key: 0u32,
			start_offset: 0u32,
		};

		self.reader
			.read_exact(&mut header.magic)
			.map_err(|e: io::Error| {
				if let io::ErrorKind::UnexpectedEof = e.kind() {
					io::Error::new(io::ErrorKind::InvalidData, "Not a HAPI archive")
				} else {
					e
				}
			})?;

		if header.magic != HAPI_MAGIC {
			return Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"Not a HAPI archive",
			));
		}

		self.reader.read_exact(&mut header.marker)?;

		if header.marker == HAPI_SAVE_MARKER {
			return Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"Save files are not supported yet",
			));
		} else if header.marker != HAPI_ARCHIVE_MARKER {
			eprintln!(
				"Warning: Unexpected value {} in header marker",
				u32::from_le_bytes(header.marker)
			);
		}

		header.size = self.reader.read_u32::<LittleEndian>()?;
		header.key = self.reader.read_u32::<LittleEndian>()?;
		header.start_offset = self.reader.read_u32::<LittleEndian>()?;
		self.start_offset = header.start_offset;

		eprintln!("debug: {:x?}", header);

		// Derive cipher key
		self.key = !((header.key * 4) | (header.key >> 6));
		eprintln!("debug: cipher key: {:x}", self.key);

		Ok(())
	}

	fn parse_toc(&mut self) -> io::Result<()> {
		unimplemented!()
	}

	pub fn open<P: AsRef<Path>>(path: P) -> io::Result<HapiArchive> {
		let file = File::open(path)?;
		let reader = BufReader::new(file);
		let key = 0u32;

		let mut file = HapiArchive {
			reader,
			key,
			start_offset: 0,
		};

		// Parse the header
		file.parse_header()?;

		Ok(file)
	}
}

impl Read for HapiArchive {
	/// Only usable once self.open() has completed (that is, all fields are populated).
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		let pos = self.seek(SeekFrom::Current(0))?;
		// NOTE: replace with stream_position when stable

		// Read bytes, store count
		let bytes_count = self.reader.read(buf)?;

		for count in 0..bytes_count {
			let offset: u32 = pos as u32 + count as u32;

			// Decipher everything except header
			if offset >= self.start_offset {
				let char_key = (offset ^ self.key) as u8;
				buf[count] = char_key ^ !buf[count];
			}
		}

		Ok(bytes_count)
	}
}

impl Seek for HapiArchive {
	fn seek(&mut self, pos: SeekFrom) -> Result<u64, io::Error> {
		self.reader.seek(pos)
	}
}

fn main() -> io::Result<()> {
	let file = HapiArchive::open("totala1.hpi")?;

	Ok(())
}
