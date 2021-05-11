use hapi::prelude::*;
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

	extract_a_file(&archive)?;

	// archive.extract_all("./testout")?;

	// list_files(&archive.contents);

	//eprintln!("{:#x?}", file);

	Ok(())
}

fn list_files(dir: &HapiDirectory) {
	println!("{}", dir.path_str());

	for entry in dir {
		match entry {
			HapiEntry::File(file) => println!("{}", file.path_str()),
			HapiEntry::Directory(dir) => list_files(dir),
		}
	}
}

fn extract_a_file(archive: &HapiArchive<File>) -> Result<(), Box<dyn Error>> {
	let file = archive
		.contents()
		.find_map(find_file)
		.expect("didn't find the file");

	archive.write_file(&file, &mut std::io::stdout())
}

fn find_file(ent: &HapiEntry) -> Option<&HapiFile> {
	match ent {
		HapiEntry::File(f) => {
			if f.path_str() == "./gamedata/SIDEDATA.TDF" {
				eprintln!("{}", f.path_str());
				Some(f)
			} else {
				None
			}
		}
		HapiEntry::Directory(d) => d.into_iter().find_map(find_file),
	}
}
