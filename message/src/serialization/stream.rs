use ser::{Stream, Serializable};
use serialization::PayloadType;
use Error;

pub struct PayloadStream {
	stream: Stream,
	version: u32,
}

impl PayloadStream {
	pub fn new(version: u32) -> Self {
		PayloadStream {
			stream: Stream::default(),
			version: version,
		}
	}

	pub fn append<T>(&mut self, t: &T) -> Result<(), Error> where T: PayloadType + Serializable {
		if self.version < T::version() {
			return Err(Error::InvalidVersion);
		}

		t.serialize(&mut self.stream);
		Ok(())
	}
}
