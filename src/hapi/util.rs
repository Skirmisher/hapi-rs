use std::io;

pub fn parse_c_string(buf: &[u8]) -> io::Result<String> {
	let end = if let Some(n) = buf.iter().position(|c| *c == 0) {
		n
	} else {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"String went past end of header",
		));
	};

	Ok(String::from_utf8_lossy(&buf[..end]).to_string())
}