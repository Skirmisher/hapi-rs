use hapi::HapiArchive;
use hapi::HapiReader;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{prelude::*, BufReader, Write};

fn main() -> Result<(), Box<dyn Error>> {
	let filename = if let Some(s) = env::args().nth(1) {
		s
	} else {
		"Example.ufo".to_string()
	};
	let file = File::open(filename)?;
	// let file = BufReader::new(file);
	// let mut file = HapiReader::new(file)?;
	// let mut contents = Vec::new();
	// file.read_to_end(&mut contents);
	// std::io::stdout().write_all(&contents);
	let file = HapiArchive::open(file)?;

	//eprintln!("{:#x?}", file);

	Ok(())
}
