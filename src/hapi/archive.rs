mod file_decoder;

use super::*;

use std::cell::RefCell;
use std::error::Error;
use std::fmt::Debug;
use std::fs::{self, File};
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};

use binrw::BinRead;

/// An existing HAPI archive.
///
/// # Examples
/// ```
/// use hapi::prelude::*;
/// use std::fs::{self, File};
/// use std::io;
///
/// // Open an existing archive file
/// let file = File::open("Example.ufo")?;
/// // Parse the archive, and get it ready for extraction
/// let archive = HapiArchive::open(file)?;
/// // Make a subdirectory to extract the contents into
/// fs::create_dir("Example")?;
/// // Extract the archive to the new subdirectory
/// archive.extract_all("Example")?;
/// // If the archive contains a "Copyright.txt" at its root, display it on stdout
/// let copyright = archive.iter().find_map(|x| {
/// 	if let HapiEntry::File(f) = x {
/// 		if f.name().eq_ignore_ascii_case("Copyright.txt") {
/// 			Some(f)
/// 		}
/// 	}
/// 	None
/// });
/// if let Some(text) = copyright {
/// 	archive.write_file(text, &mut io::stdout())?
/// }
/// ```
#[derive(Debug)]
pub struct HapiArchive<R: Read + Seek> {
	reader: RefCell<HapiReader<R>>,
	/// The root directory as a `HapiDirectory`, for completeness. Usually you'll
	/// just want [`contents`](Self::contents), which is a shortcut for [`root_dir.iter`].
	///
	/// [`root_dir.iter`]: HapiDirectory::iter
	pub root_dir: HapiDirectory,
}

impl<'a> IntoIterator for &'a HapiDirectory {
	type Item = &'a HapiEntry;
	type IntoIter = std::slice::Iter<'a, HapiEntry>;

	fn into_iter(self) -> Self::IntoIter {
		self.contents.iter()
	}
}

impl HapiDirectory {
	/// Returns the directory's full path within the archive, relative to the archive root
	/// (denoted by `./`).
	pub fn path(&self) -> &Path {
		self.path.as_path()
	}

	/// Returns the result of [`path`](Self::path()) as a `&str`.
	pub fn path_str(&self) -> &str {
		self.path.to_str().unwrap()
	}

	/// Returns just the directory name as a `&str`.
	pub fn name(&self) -> &str {
		// SAFETY: NullString -> String conversion already replaced invalid UTF-8
		self.path.file_name().map_or("", |s| s.to_str().unwrap())
	}

	/// Returns the entries under this directory.
	pub fn iter(&self) -> <&HapiDirectory as IntoIterator>::IntoIter {
		self.into_iter()
	}
}

impl HapiFile {
	/// Returns the file's full path within the archive, relative to the archive root
	/// (denoted by `./`).
	pub fn path(&self) -> &Path {
		self.path.as_path()
	}

	/// Returns the result of [`path`](Self::path()) as a `&str`.
	pub fn path_str(&self) -> &str {
		self.path.to_str().unwrap()
	}

	/// Returns just the file name as a `&str`.
	pub fn name(&self) -> &str {
		// SAFETY: NullString -> String conversion already replaced invalid UTF-8
		self.path.file_name().map_or("", |s| s.to_str().unwrap())
	}
}

impl HapiEntry {
	/// Returns `Some(file)` if this entry holds a file; otherwise returns `None`.
	pub fn as_file(&self) -> Option<&HapiFile> {
		if let HapiEntry::File(file) = self {
			Some(file)
		} else {
			None
		}
	}

	/// Returns `Some(dir)` if this entry holds a directory; otherwise returns `None`.
	pub fn as_dir(&self) -> Option<&HapiDirectory> {
		if let HapiEntry::Directory(dir) = self {
			Some(dir)
		} else {
			None
		}
	}
}

impl<R> HapiArchive<R>
where
	R: Read + Seek + Debug,
{
	/// Opens an existing archive for reading.
	///
	/// Once you've opened an archive, you can iterate over its entries with [`contents`] and
	/// pass them to this struct's methods as necessary (or just call [`extract_all`]).
	///
	/// [`contents`]: Self::contents
	/// [`extract_all`]: Self::extract_all
	pub fn open(stream: R) -> Result<HapiArchive<R>, Box<dyn Error>> {
		// Create reader
		let mut reader = HapiReader::new(stream)?;

		// Parse table of contents
		reader.seek(SeekFrom::Start(reader.header.toc_offset as u64))?;
		let contents = HapiDirectory::read_args(&mut reader, (PathBuf::from("."),))?;

		Ok(HapiArchive {
			reader: RefCell::new(reader),
			root_dir: contents,
		})
	}

	/// Returns an iterator over the entries in the archive's root directory.
	pub fn contents(&self) -> <&HapiDirectory as IntoIterator>::IntoIter {
		self.root_dir.iter()
	}

	/// Extracts a file from the archive into the directory denoted by `dest`.
	///
	/// If a file with the same name as `entry` already exists in `dest`, it will be
	/// truncated and overwritten.
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

	/// Writes a file from the archive to an arbitrary output stream.
	///
	/// This is useful for writing to stdout, for example. If you want the library
	/// to create the file with the correct name for you, use
	/// [`extract_file`](Self::extract_file).
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

	/// Extracts the entire contents of the archive into the directory specified by `dest`.
	///
	/// A shortcut for `archive.extract_dir(archive.root_dir, dest)`.
	pub fn extract_all(&self, dest: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
		self.extract_dir(&self.root_dir, dest)
	}

	/// Extracts the contents of the archive under `dir` into the directory specified by `dest`.
	///
	/// Note that the directory itself is not created within `dest`, only its contents.
	/// Any existing files that collide with a file from the archive will be overwritten.
	pub fn extract_dir(
		&self,
		dir: &HapiDirectory,
		dest: impl AsRef<Path>,
	) -> Result<(), Box<dyn Error>> {
		if !dest.as_ref().metadata()?.is_dir() {
			return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a directory").into());
		}

		eprintln!("Extracting to {}", dest.as_ref().to_str().unwrap());

		for entry in dir {
			match entry {
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
