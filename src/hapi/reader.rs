use std::error::Error;
use std::io::{self, prelude::*, ErrorKind, SeekFrom};

use binrw::BinRead;

use super::*;

#[derive(Debug)]
pub struct HapiReader<R: Read + Seek> {
	inner: R,
	pub(super) header: HapiHeader,
}

impl<R> HapiReader<R>
where
	R: Read + Seek,
{
	pub fn new(mut inner: R) -> Result<HapiReader<R>, Box<dyn Error>> {
		// Parse header
		let header = HapiHeader::read(&mut inner).map_err(|e| -> Box<dyn Error> {
			if let binrw::error::Error::BadMagic { .. } = e {
				io::Error::new(ErrorKind::InvalidData, "Not a HAPI archive").into()
			} else {
				e.into()
			}
		})?;

		if header.marker == HAPI_SAVE_MARKER {
			return Err(
				io::Error::new(ErrorKind::InvalidData, "Save data is not supported yet").into(),
			);
		} else if header.marker != HAPI_ARCHIVE_MARKER {
			// XXX how 2 warn from library
			eprintln!(
				"Warning: Unknown header marker {:x?}. Proceeding without caution.",
				header.marker
			);
		}

		Ok(HapiReader { inner, header })
	}
}

// Trait impls

impl<R> Read for HapiReader<R>
where
	R: Read + Seek,
{
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		let pos = self.stream_position()?;

		// Read bytes, store count
		let bytes_count = self.inner.read(buf)?;

		// Decipher if key is present
		if let Some(key) = self.header.key {
			for (count, byte) in buf.iter_mut().enumerate().take(bytes_count) {
				let offset = pos as u32 + count as u32;

				// Decipher everything except header
				if offset >= self.header.toc_offset {
					// This is where the magic happens
					let char_key = (offset ^ key) as u8;
					*byte = char_key ^ !*byte;
				}
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
