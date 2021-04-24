use hapi::{HapiArchive, HapiFile, HapiDirectory, HapiEntry, HapiEntryIndex};
use std::env;
use std::error::Error;
use std::fs::File;

fn main() -> Result<(), Box<dyn Error>> {
	let filename = if let Some(s) = env::args().nth(1) {
		s
	} else {
		"Example.ufo".to_string()
	};
	let archive = File::open(filename)?;
	// let file = BufReader::new(file);
	// let mut file = HapiReader::new(file)?;
	// let mut contents = Vec::new();
	// file.read_to_end(&mut contents);
	// std::io::stdout().write_all(&contents);
	let archive = HapiArchive::open(archive)?;

	// extract_first_file(&archive)?;

	archive.extract_all("./testout")?;

	// list_files(&archive.contents);

	//eprintln!("{:#x?}", file);

	Ok(())
}

fn list_files(dir: &HapiDirectory) {
	println!("{}", dir.path_str());

	for index in &dir.contents {
		match &index.entry {
			HapiEntry::File(file) => println!("{}", file.path_str()),
			HapiEntry::Directory(dir) => list_files(dir),
		}
	}
}

fn extract_first_file(archive: &HapiArchive<File>) -> Result<(), Box<dyn Error>> {
	let file = archive
		.contents
		.contents
		.iter()
		.find_map(find_file)
		.expect("no files in archive");

	archive.write_file(&file, &mut std::io::stdout())
}

fn find_file(ent: &HapiEntryIndex) -> Option<&HapiFile> {
	match &ent.entry {
		HapiEntry::File(f) => {
			eprintln!("{}", f.path.to_string_lossy());
			Some(f)
		}
		HapiEntry::Directory(d) => d.contents.iter().find_map(find_file),
	}
}
