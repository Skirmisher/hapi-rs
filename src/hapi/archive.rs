use std::fs::{File, ReadDir};
use std::io::{self, prelude::*, BufReader, Error, ErrorKind};
use std::path::{Path, PathBuf};

use super::reader::{HapiContents, HapiDirectory, HapiEntry, HapiFile, HapiReader};

const HAPI_CHUNK_SIZE: usize = 65536;

enum OutputTarget<W: Write> {
	Stream(W),
	Directory(PathBuf),
}

pub struct HapiArchive<R: Read + Seek> {
	reader: HapiReader<R>,
	contents: HapiContents,
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

	fn extract_entry<W: Write>(
		&mut self,
		entry: &HapiEntry,
		output: &mut OutputTarget<W>,
	) -> io::Result<()> {
		match entry {
			HapiEntry::File(file) => {
				match output {
					OutputTarget::Stream(output) => self.extract_file(&file, output),
					OutputTarget::Directory(path) => {
						if path.is_dir() {
							let output = File::create(path.with_file_name(&file.name))?;
							self.extract_file(&file, output)
						} else {
							Err(Error::new(ErrorKind::InvalidInput, "Not a directory"))
						}
					}
				}
			}
			HapiEntry::Directory(directory) => {
				if let OutputTarget::Stream(_) = output {
					panic!("fucko");
				}
				for entry in &directory.contents {
					self.extract_entry(&entry, output)?;
				}
				Ok(())
			}
		}
	}

	fn extract_file<W: Write>(&mut self, entry: &HapiFile, output: W) -> io::Result<()> {
		unimplemented!()
	}
}
