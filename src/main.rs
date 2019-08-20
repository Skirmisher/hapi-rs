use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{self, prelude::*, BufReader, SeekFrom};
use std::path::Path;

const HAPI_MAGIC: &[u8] = b"HAPI";
const HAPI_SAVE_MARKER: &[u8] = b"BANK";
const HAPI_ARCHIVE_MARKER: &[u8] = &[0x00, 0x00, 0x01, 0x00];

pub struct HapiArchive<R: Read + Seek> {
	reader: HapiReader<R>,
	contents: HapiContents,
}

struct HapiReader<R: Read + Seek> {
	inner: BufReader<R>,
	key: u32,
	start_offset: usize,
	root_entry_size: usize,
}

enum HapiEntry {
	File {
		name: String,
		offset: usize,
		extracted_size: usize,
		compression: HapiCompressionType,
	},
	Directory {
		name: String,
		contents: Vec<HapiEntry>,
	},
}

enum HapiCompressionType {
	None,
	Lz77,
	Deflate,
}

type HapiContents = HapiEntry;

// HAPI header structure: 20 bytes
#[derive(Debug)]
struct HapiHeader {
	magic: [u8; 4],    // HAPI_MAGIC
	marker: [u8; 4],   // HAPI_SAVE_MARKER or HAPI_ARCHIVE_MARKER
	size: u32,         // size of table of contents
	key: u32,          // XOR cipher key
	start_offset: u32, // offset of contents from file start
}

impl<R> HapiArchive<R>
where
	R: Read + Seek,
{
	pub fn open(stream: R) -> io::Result<HapiArchive<R>> {
		let reader = BufReader::new(stream);

		// Create reader
		let mut reader = HapiReader::new(reader)?;

		// Parse table of contents
		let contents = reader.parse_toc()?;

		Ok(HapiArchive { reader, contents })
	}
}

impl<R> HapiReader<R>
where
	R: Read + Seek,
{
	fn new(mut inner: BufReader<R>) -> io::Result<HapiReader<R>> {
		// Parse header
		let header = HapiReader::parse_header(&mut inner)?;
		eprintln!("Debug: {:x?}", header);

		// Derive cipher key
		let key = !((header.key * 4) | (header.key >> 6));
		eprintln!("Debug: cipher key: {:x}", key);

		Ok(HapiReader {
			inner,
			key,
			start_offset: header.start_offset as usize,
			root_entry_size: header.size as usize,
		})
	}

	fn parse_header(reader: &mut BufReader<R>) -> io::Result<HapiHeader> {
		let mut header = HapiHeader {
			magic: [0u8; 4],
			marker: [0u8; 4],
			size: 0u32,
			key: 0u32,
			start_offset: 0u32,
		};

		reader
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

		reader.read_exact(&mut header.marker)?;

		if header.marker == HAPI_SAVE_MARKER {
			return Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"Save data is not supported yet",
			));
		} else if header.marker != HAPI_ARCHIVE_MARKER {
			eprintln!(
				"Warning: Unexpected value {} in header marker",
				u32::from_le_bytes(header.marker)
			);
		}

		header.size = reader.read_u32::<LittleEndian>()?;
		header.key = reader.read_u32::<LittleEndian>()?;
		header.start_offset = reader.read_u32::<LittleEndian>()?;

		Ok(header)
	}

	fn parse_toc(&mut self) -> io::Result<HapiContents> {
		let mut toc = vec![0u8; self.root_entry_size + self.start_offset];
		self.read_exact(&mut toc[self.start_offset..])?;
		//eprintln!("Debug: table of contents: {:x}", &toc);

		self.parse_contents(&toc, self.start_offset)
	}

	fn parse_contents(&mut self, buf: &Vec<u8>, offset: usize) -> io::Result<HapiEntry> {
		unimplemented!()
	}
}

// Trait impls

impl<R> Read for HapiReader<R>
where
	R: Read + Seek,
{
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		let pos = self.seek(SeekFrom::Current(0))?;
		// NOTE: replace with stream_position when stable

		// Read bytes, store count
		let bytes_count = self.inner.read(buf)?;

		for count in 0..bytes_count {
			let offset = pos as usize + count;

			// Decipher everything except header
			if offset >= self.start_offset {
				let char_key = (offset as u32 ^ self.key) as u8;
				buf[count] = char_key ^ !buf[count];
			}
		}

		Ok(bytes_count)
	}
}

impl<R> Seek for HapiReader<R>
where
	R: Read + Seek,
{
	fn seek(&mut self, pos: SeekFrom) -> Result<u64, io::Error> {
		self.inner.seek(pos)
	}
}

fn main() -> io::Result<()> {
	let file = File::open("totala1.hpi")?;
	let file = HapiArchive::open(file)?;

	Ok(())
}
