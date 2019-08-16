use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::string::String;

const HAPI_HEADER_LEN: usize = 20;

struct HapiFile {
    reader: BufReader<File>,
}

impl HapiFile {
    fn open<P: AsRef<Path>>(path: P) -> io::Result<HapiFile> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut header = [0u8; HAPI_HEADER_LEN];

        reader.read(&mut header)?;
        let mut magic = String::from_utf8_lossy(&header[0..4]);
        if magic != "HAPI" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Not a HAPI archive",
            ));
        }

        Ok(HapiFile { reader })
    }
}

fn main() -> io::Result<()> {
    let file = HapiFile::open("totala1.hpi")?;

    Ok(())
}
