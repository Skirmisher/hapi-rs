use std::io;
use std::fs::File;
use std::env;
use hapi::HapiArchive;

fn main() -> io::Result<()> {
	let filename = if let Some(s) = env::args().nth(1) {
		s
	} else {
		"totala1.hpi".to_string()
	};
	let file = File::open(filename)?;

	/*
	let reader = BufReader::new(file);
	let mut reader = hapi::HapiReader::new(reader)?;
	let mut buf = [0u8; 59285];
	reader.read_exact(&mut buf);
	io::stdout().write_all(&buf)?;
	*/

	let file = HapiArchive::open(file)?;

	Ok(())
}
