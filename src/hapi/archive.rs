use std::error::Error;
use std::fs::{File, ReadDir};
use std::io::{self, prelude::*, BufReader, ErrorKind};
use std::path::{Path, PathBuf};

use binread::BinRead;

use super::*;

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
	R: Read + Seek,
{
	pub fn open(stream: R) -> Result<HapiArchive<R>, Box<dyn Error>> {
		// Create reader
		let reader = BufReader::new(stream);
		let mut reader = HapiReader::new(reader)?;

		// Parse table of contents
		reader.seek(SeekFrom::Start(reader.header.toc_offset as u64))?;
		let contents = HapiDirectory::read(&mut reader)?;

		Ok(HapiArchive { reader, contents })
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

	fn extract_file<W: Write>(&mut self, entry: &HapiFile, output: &mut W) -> io::Result<()> {
		todo!()
	}
}
