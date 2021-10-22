#![allow(clippy::tabs_in_doc_comments)] // really?

mod hapi;

pub use hapi::*;

pub mod prelude {
	#[doc(no_inline)]
	pub use crate::{HapiArchive, HapiCompressionType, HapiDirectory, HapiEntry, HapiFile};
}
