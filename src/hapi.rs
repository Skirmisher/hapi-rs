mod archive;
mod reader;

pub use self::archive::*;
pub use self::reader::*;

// =^w^= =^w^= =^w^= =^w^= =^w^=
// ~* common data structures *~
// =^w^= =^w^= =^w^= =^w^= =^w^=

use binread::{derive_binread, io::SeekFrom, prelude::*, FilePtr32, NullString};

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
#[derive(Debug)]
#[br(little)]
struct HapiDirectory {
	#[br(temp)]
	count: u32,
	#[br(parse_with = FilePtr32::parse, count = count)]
	contents: Vec<HapiEntryIndex>,
}

// Index: names entry, points to either file or directory data
#[derive_binread]
#[derive(Debug)]
#[br(little)]
struct HapiEntryIndex {
	#[br(parse_with = FilePtr32::parse, map = |str: NullString| str.into_string())]
	name: String,
	#[br(seek_before = SeekFrom::Current(4), restore_position, temp, map = |flag: u8| flag == 1)]
	is_dir: bool, // this comes after the entry pointer but we need to pass it to HapiEntry
	#[br(parse_with = FilePtr32::parse, args(is_dir), pad_after = 1)]
	entry: HapiEntry,
}

// Entry: either file or directory
#[derive(Debug, BinRead)]
#[br(little, import(is_dir: bool))]
enum HapiEntry {
	#[br(pre_assert(!is_dir))]
	File(HapiFile),
	#[br(pre_assert(is_dir))]
	Directory(HapiDirectory),
}

// File entry
// Compressed case: points to array of chunks
// Uncompressed case: points to contiguous file data
#[derive(Debug, BinRead)]
#[br(little)]
struct HapiFile {
	// placeholder args since we are not reading file contents yet
	#[br(parse_with = FilePtr32::read_options, args(0, HapiCompressionType::None))]
	contents: FilePtr32<HapiFileContents>,
	extracted_size: u32,
	compression: HapiCompressionType,
}

// How a file is compressed (or not)
#[derive(Debug, BinRead, PartialEq, Clone, Copy)]
#[br(repr(u8))]
enum HapiCompressionType {
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
		#[br(temp, calc = extracted_size / HAPI_CHUNK_SIZE + (extracted_size % HAPI_CHUNK_SIZE != 0) as u32)]
		 u32, // number of chunks
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
			data.iter().fold(0, |c: u32, i: &u8| c.wrapping_add(*i as u32))),
		temp
	)]
	data: Vec<u8>,
}
