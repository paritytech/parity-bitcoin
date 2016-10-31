use ser::Reader;
use {Payload, Error};

pub fn deserialize_payload<T>(buffer: &[u8], version: u32) -> Result<T, Error> where T: Payload {
	let mut reader = PayloadReader::new(buffer, version);
	let result = try!(reader.read());
	if !reader.is_finished() {
		return Err(Error::Deserialize);
	}

	Ok(result)
}

pub struct PayloadReader<T> {
	reader: Reader<T>,
	version: u32,
}

impl<'a> PayloadReader<&'a [u8]> {
	pub fn new(buffer: &'a [u8], version: u32) -> Self {
		PayloadReader {
			reader: Reader::new(buffer),
			version: version,
		}
	}

	pub fn read<T>(&mut self) -> Result<T, Error> where T: Payload {
		if T::version() > self.version {
			return Err(Error::InvalidVersion);
		}

		T::deserialize_payload(&mut self.reader, self.version)
	}

	pub fn is_finished(&mut self) -> bool {
		self.reader.is_finished()
	}
}
