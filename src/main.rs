use hapi::HapiArchive;
// use hapi::HapiReader;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn Error>> {
	let filename = if let Some(s) = env::args().nth(1) {
		s
	} else {
		"Example.ufo".to_string()
	};
	let file = File::open(filename)?;
	// let file = BufReader::new(file);
	// let file = HapiReader::new(file)?;
	let file = HapiArchive::open(file)?;

	eprintln!("{:#x?}", file);

	Ok(())
}
