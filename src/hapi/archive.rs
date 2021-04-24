mod file_decoder;

use super::*;

use std::cell::RefCell;
use std::error::Error;
use std::fmt::Debug;
use std::fs::{self, File, FileType, Metadata, ReadDir};
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};

use binrw::BinRead;

#[derive(Debug)]
pub struct HapiArchive<R: Read + Seek> {
	pub reader: RefCell<HapiReader<R>>,
	pub contents: HapiDirectory,
}

impl HapiDirectory {
	pub fn path_str(&self) -> &str {
		self.path.to_str().unwrap()
	}

	pub fn name(&self) -> &str {
		// SAFETY: NullString -> String conversion already replaced invalid UTF-8
		self.path.file_name().map_or("", |s| s.to_str().unwrap())
	}
}

impl HapiFile {
	pub fn path_str(&self) -> &str {
		self.path.to_str().unwrap()
	}

	pub fn name(&self) -> &str {
		// SAFETY: NullString -> String conversion already replaced invalid UTF-8
		self.path.file_name().map_or("", |s| s.to_str().unwrap())
	}
}

impl<R> HapiArchive<R>
where
	R: Read + Seek + Debug,
{
	pub fn open(stream: R) -> Result<HapiArchive<R>, Box<dyn Error>> {
		// Create reader
		let mut reader = HapiReader::new(stream)?;

		// Parse table of contents
		reader.seek(SeekFrom::Start(reader.header.toc_offset as u64))?;
		let contents = HapiDirectory::read_args(&mut reader, (PathBuf::from(".").into(),))?;

		Ok(HapiArchive {
			reader: RefCell::new(reader),
			contents,
		})
	}

	pub fn extract_file(
		&self,
		entry: &HapiFile,
		dest: impl AsRef<Path>,
	) -> Result<(), Box<dyn Error>> {
		if !dest.as_ref().metadata()?.is_dir() {
			return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a directory").into());
		}

		let filename = dest.as_ref().join(entry.name());

		eprintln!("Creating file {}", filename.to_str().unwrap());

		let mut file = File::create(filename)?;

		self.write_file(entry, &mut file)
	}

	pub fn write_file(
		&self,
		entry: &HapiFile,
		output: &mut impl Write,
	) -> Result<(), Box<dyn Error>> {
		self.reader
			.borrow_mut()
			.seek(SeekFrom::Start(entry.contents_offset as u64))?;
		let contents = HapiFileContents::read_args(
			&mut *self.reader.borrow_mut(),
			(entry.extracted_size, entry.compression),
		)?;

		match contents {
			HapiFileContents::Uncompressed(data) => Ok(output.write_all(&data)?),
			HapiFileContents::Compressed(chunks, ..) => {
				chunks.iter().try_for_each(|chunk| chunk.decompress(output))
			}
		}
	}

	pub fn extract_all(&self, dest: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
		self.extract_dir(&self.contents, dest)
	}

	pub fn extract_dir(
		&self,
		dir: &HapiDirectory,
		dest: impl AsRef<Path>,
	) -> Result<(), Box<dyn Error>> {
		if !dest.as_ref().metadata()?.is_dir() {
			return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a directory").into());
		}

		eprintln!("Extracting to {}", dest.as_ref().to_str().unwrap());

		for ent in &dir.contents {
			match &ent.entry {
				HapiEntry::File(file) => self.extract_file(file, dest.as_ref())?,
				HapiEntry::Directory(dir) => {
					let dest = dest.as_ref().join(dir.name()); // FIXME check for errant path separators
					eprintln!("Creating dir {}", dest.to_str().unwrap());
					fs::create_dir_all(&dest)?;
					self.extract_dir(&dir, dest)?;
				}
			}
		}

		Ok(())
	}
}
