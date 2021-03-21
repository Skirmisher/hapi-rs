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

#[derive_binread]
#[derive(Debug)]
#[br(little)]
struct HapiDirectory {
	#[br(temp)]
	count: u32,
	#[br(parse_with = FilePtr32::parse, count = count)]
	contents: Vec<HapiEntryIndex>,
}

#[derive_binread]
#[derive(Debug)]
#[br(little)]
struct HapiEntryIndex {
	#[br(parse_with = FilePtr32::parse, map = |str: NullString| str.into_string())]
	name: String,
	#[br(seek_before = SeekFrom::Current(4), restore_position, temp, map = |flag: u8| flag == 1)]
	is_dir: bool, // this comes after the entry pointer but we need to pass it to HapiEntry
	#[br(parse_with = FilePtr32::parse, args(is_dir))]
	entry: HapiEntry,
	#[br(temp)]
	_is_dir: u8, // actually consume the flag for alignment
}

#[derive(Debug, BinRead)]
#[br(little, import(is_dir: bool))]
enum HapiEntry {
	#[br(pre_assert(!is_dir))]
	File(HapiFile),
	#[br(pre_assert(is_dir))]
	Directory(HapiDirectory),
}

#[derive(Debug, BinRead)]
#[br(little)]
struct HapiFile {
	offset: FilePtr32<u8>,
	extracted_size: u32,
	compression: HapiCompressionType,
}

#[derive(Debug, BinRead)]
#[br(repr(u8))]
enum HapiCompressionType {
	None = 0,
	Lz77,
	Zlib,
}
