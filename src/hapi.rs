mod archive;
mod reader;

pub use self::archive::*;
use self::reader::*;

// =^w^= =^w^= =^w^= =^w^= =^w^=
// ~* common data structures *~
// =^w^= =^w^= =^w^= =^w^= =^w^=

use std::path::PathBuf;

use binrw::{binread, prelude::*, FilePtr32, NullString, ReadOptions};
use std::io::{Read, Seek, SeekFrom};

const _HAPI_MAGIC: &[u8] = b"HAPI";
const HAPI_SAVE_MARKER: &[u8] = b"BANK";
const HAPI_ARCHIVE_MARKER: &[u8] = &[0x00, 0x00, 0x01, 0x00];
const HAPI_CHUNK_SIZE: u32 = 65536;

// HAPI header structure: 20 bytes (including magic)
#[derive(Debug, BinRead, Clone)]
#[br(little, magic = b"HAPI")]
struct HapiHeader {
	marker: [u8; 4], // HAPI_SAVE_MARKER or HAPI_ARCHIVE_MARKER
	toc_size: u32,   // size of table of contents
	#[br(map = |key: u32| if key == 0 { None } else { Some( !((key * 4) | (key >> 6)) ) })]
	key: Option<u32>, // XOR cipher key
	toc_offset: u32, // root directory of archive
}

// Directory: array of indexes to Entries
/// A directory within a [`HapiArchive`].
#[binread]
#[derive(Debug, Clone)]
#[br(little, import(path: PathBuf))]
pub struct HapiDirectory {
	#[br(calc = path)]
	path: PathBuf,
	#[br(temp)]
	count: u32,
	#[br(parse_with = FilePtr32::parse, args { count: count as usize, inner: (path.clone(),) })]
	contents: Vec<HapiEntry>,
}

// Index: names entry, points to either file or directory data
#[derive(Debug, BinRead, Clone)]
#[br(little)]
struct HapiEntryIndex {
	#[br(parse_with = FilePtr32::parse)]
	name: NullString,
	entry_offset: u32,
	#[br(map = |flag: u8| flag == 1)]
	is_dir: bool,
}

// Entry: either file or directory
/// An entry within a [`HapiArchive`]: either a file or a directory.
#[derive(Debug, Clone)]
pub enum HapiEntry {
	File(HapiFile),
	Directory(HapiDirectory),
}

impl BinRead for HapiEntry {
	type Args = (PathBuf,);

	fn read_options<R: Read + Seek>(
		reader: &mut R,
		options: &ReadOptions,
		args: Self::Args,
	) -> BinResult<Self> {
		let index = HapiEntryIndex::read_options(reader, options, ())?;

		let mut path = args.0;
		// FIXME this will MISBEHAVE if `name` is empty or weird (e.g. "..")
		path.push(index.name.into_string());

		let old_pos = SeekFrom::Start(reader.stream_position()?);
		reader.seek(SeekFrom::Start(index.entry_offset as u64))?;

		let entry = if index.is_dir {
			HapiEntry::Directory(HapiDirectory::read_options(reader, options, (path,))?)
		} else {
			HapiEntry::File(HapiFile::read_options(reader, options, (path,))?)
		};

		reader.seek(old_pos)?;
		Ok(entry)
	}
}

// File entry
// Compressed case: points to array of chunks
// Uncompressed case: points to contiguous file data
/// A file within a [`HapiArchive`].
#[derive(Debug, BinRead, Clone)]
#[br(little, import(path: PathBuf))]
pub struct HapiFile {
	#[br(calc = path)]
	path: PathBuf,
	/// Where the file starts within the archive. (The contents at this location
	/// depend on if it's compressed or not.)
	pub contents_offset: u32,
	/// The size of the decompressed file, in bytes, as reported by the archive.
	pub extracted_size: u32,
	/// How the file is compressed, if at all.
	pub compression: HapiCompressionType,
}

// How a file is compressed (or not)
/// A [`HapiFile`]'s compression scheme, or lack thereof.
#[derive(Debug, BinRead, PartialEq, Clone, Copy)]
#[br(repr(u8))]
pub enum HapiCompressionType {
	None = 0,
	Lz77,
	Zlib,
}

// The target of a File entry: either uncompressed data, or a series of compressed chunks
#[binread]
#[derive(Debug)]
#[br(little, import(extracted_size: u32, compression: HapiCompressionType))]
enum HapiFileContents {
	#[br(pre_assert(compression == HapiCompressionType::None))]
	Uncompressed(#[br(count = extracted_size)] Vec<u8>),
	#[br(pre_assert(compression != HapiCompressionType::None))]
	Compressed(
		#[br(temp, calc = (extracted_size + HAPI_CHUNK_SIZE - 1) / HAPI_CHUNK_SIZE)] u32, // number of chunks
		#[br(temp, count = self_0)] Vec<u32>, // size of each chunk (unnecessary here)
		#[br(count = self_0)] Vec<HapiCompressedChunk>, // the chunks themselves
	),
}

// Header preceding a chunk of compressed data
#[binread]
#[derive(Debug)]
#[br(little, magic = b"SQSH")]
struct HapiCompressedChunk {
	#[br(temp)]
	_version_maybe: u8,
	#[br(assert(compression != HapiCompressionType::None))]
	compression: HapiCompressionType,
	#[br(map = |flag: u8| flag == 1)]
	is_enciphered: bool,
	compressed_size: u32,
	decompressed_size: u32,
	checksum: u32,
	#[br(
		count = compressed_size,
		assert(
			data.iter().fold(0, |c: u32, i: &u8| c.wrapping_add(*i as u32)) == checksum,
			"Chunk had bad checksum (expected {:x}, actual was {:x})",
			checksum,
			data.iter().fold(0, |c: u32, i: &u8| c.wrapping_add(*i as u32))
		)
	)]
	data: Vec<u8>,
}
