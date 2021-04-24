mod archive;
mod reader;

pub use self::archive::*;
pub use self::reader::*;

// =^w^= =^w^= =^w^= =^w^= =^w^=
// ~* common data structures *~
// =^w^= =^w^= =^w^= =^w^= =^w^=

use binrw::{derive_binread, io::SeekFrom, prelude::*, FilePtr32, NullString};
use std::path::PathBuf;
use std::rc::Rc;

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
#[derive_binread]
#[derive(Debug, Clone)]
#[br(little, import(path: Rc<PathBuf>))]
pub struct HapiDirectory {
	#[br(calc = path.clone())]
	pub path: Rc<PathBuf>,
	#[br(temp)]
	count: u32,
	#[br(parse_with = FilePtr32::parse, count = count, args(path.clone()))]
	pub contents: Vec<HapiEntryIndex>,
}

// Index: names entry, points to either file or directory data
#[derive_binread]
#[derive(Debug, Clone)]
#[br(little, import(parent: Rc<PathBuf>))]
pub struct HapiEntryIndex {
	#[br(parse_with = FilePtr32::parse, map = |str: NullString| parent.join(str.into_string()).into(), temp)]
	path: Rc<PathBuf>,
	#[br(seek_before = SeekFrom::Current(4), restore_position, temp, map = |flag: u8| flag == 1)]
	is_dir: bool, // this comes after the entry pointer but we need to pass it to HapiEntry
	#[br(parse_with = FilePtr32::parse, args(is_dir, path), pad_after = 1)]
	pub entry: HapiEntry,
}

// Entry: either file or directory
#[derive(Debug, BinRead, Clone)]
#[br(little, import(is_dir: bool, path: Rc<PathBuf>))]
pub enum HapiEntry {
	#[br(pre_assert(!is_dir))]
	File(#[br(args(path.clone()))] HapiFile),
	#[br(pre_assert(is_dir))]
	Directory(#[br(args(path.clone()))] HapiDirectory),
}

// File entry
// Compressed case: points to array of chunks
// Uncompressed case: points to contiguous file data
#[derive(Debug, BinRead, Clone)]
#[br(little, import(path: Rc<PathBuf>))]
pub struct HapiFile {
	#[br(calc = path)]
	pub path: Rc<PathBuf>,
	pub contents_offset: u32,
	pub extracted_size: u32,
	pub compression: HapiCompressionType,
}

// How a file is compressed (or not)
#[derive(Debug, BinRead, PartialEq, Clone, Copy)]
#[br(repr(u8))]
pub enum HapiCompressionType {
	None = 0,
	Lz77,
	Zlib,
}

// The target of a File entry: either uncompressed data, or a series of compressed chunks
#[derive_binread]
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
#[derive_binread]
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
