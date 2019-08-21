use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{self, prelude::*, BufReader, SeekFrom};

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

#[derive(Debug)]
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

#[derive(Debug)]
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
		eprintln!("Debug: directory tree: {:#?}", contents);

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
				"Warning: Unexpected value {:x} in header marker",
				u32::from_le_bytes(header.marker)
			);
		}

		header.size = reader.read_u32::<LittleEndian>()?;
		header.key = reader.read_u32::<LittleEndian>()?;
		header.start_offset = reader.read_u32::<LittleEndian>()?;

		Ok(header)
	}

	fn parse_toc(&mut self) -> io::Result<HapiContents> {
		// Allocate buffer
		let mut toc = vec![0u8; self.root_entry_size];

		// Copy into buffer starting at start_offset so pointers are aligned
		self.read_exact(&mut toc[self.start_offset..])?;
		eprintln!("Debug: table of contents: {:x?}", &toc);

		// Begin recursion
		self.parse_directory(String::from("."), &toc, self.start_offset)
	}

	fn parse_directory(
		&mut self,
		name: String,
		buf: &Vec<u8>,
		offset: usize,
	) -> io::Result<HapiEntry> {
		// Begin reading at offset
		let mut buf_slice = &buf[offset..];

		// Construct the vector of entries
		let num_entries = buf_slice.read_u32::<LittleEndian>()? as usize;
		let mut entries = Vec::with_capacity(num_entries);

		// Jump to list of entries
		let offset = buf_slice.read_u32::<LittleEndian>()? as usize;
		let mut buf_slice = &buf[offset..];

		// Parse entries
		for _ in 0..num_entries {
			let name_offset = buf_slice.read_u32::<LittleEndian>()? as usize;
			let name = util::parse_c_string(&buf[name_offset..])?;

			let entry_offset = buf_slice.read_u32::<LittleEndian>()? as usize;

			let is_directory = buf_slice.read_u8()? != 0;
			entries.push(if is_directory {
				self.parse_directory(name, buf, entry_offset)?
			} else {
				self.parse_file(name, buf, entry_offset)?
			});
		}

		Ok(HapiEntry::Directory {
			name,
			contents: entries,
		})
	}

	fn parse_file(&mut self, name: String, buf: &Vec<u8>, offset: usize) -> io::Result<HapiEntry> {
		let mut buf = &buf[offset..];

		let offset = buf.read_u32::<LittleEndian>()? as usize;
		let extracted_size = buf.read_u32::<LittleEndian>()? as usize;
		let compression = buf.read_u8()?;
		let compression = match compression {
			0 => HapiCompressionType::None,
			1 => HapiCompressionType::Lz77,
			2 => HapiCompressionType::Deflate,
			_ => {
				return Err(io::Error::new(
					io::ErrorKind::InvalidData,
					format!(
						"Invalid compression type {} found for file {}",
						compression, name
					),
				))
			}
		};

		Ok(HapiEntry::File {
			name,
			offset,
			extracted_size,
			compression,
		})
	}
}

// Trait impls

impl<R> Read for HapiReader<R>
where
	R: Read + Seek,
{
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
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
	fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
		self.inner.seek(pos)
	}
}

mod util {
	pub fn parse_c_string(buf: &[u8]) -> std::io::Result<String> {
		let end = if let Some(n) = buf.iter().position(|c| *c == 0) {
			n
		} else {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"String went past end of header",
			));
		};

		Ok(String::from_utf8_lossy(&buf[..end]).to_string())
	}
}

fn main() -> io::Result<()> {
	let file = File::open("totala1.hpi")?;
	let file = HapiArchive::open(file)?;

	Ok(())
}
