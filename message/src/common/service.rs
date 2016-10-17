use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError
};

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub struct Services(u64);

impl From<Services> for u64 {
	fn from(s: Services) -> Self {
		s.0
	}
}

impl From<u64> for Services {
	fn from(v: u64) -> Self {
		Services(v)
	}
}


impl Services {
	pub fn network(&self) -> bool {
		self.bit_at(0)
	}

	pub fn with_network(mut self, v: bool) -> Self {
		self.set_bit(0, v);
		self
	}

	pub fn getutxo(&self) -> bool {
		self.bit_at(1)
	}

	pub fn with_getutxo(mut self, v: bool) -> Self {
		self.set_bit(1, v);
		self
	}

	pub fn bloom(&self) -> bool {
		self.bit_at(2)
	}

	pub fn with_bloom(mut self, v: bool) -> Self {
		self.set_bit(2, v);
		self
	}

	pub fn witness(&self) -> bool {
		self.bit_at(3)
	}

	pub fn with_witness(mut self, v: bool) -> Self {
		self.set_bit(3, v);
		self
	}

	pub fn xthin(&self) -> bool {
		self.bit_at(4)
	}

	pub fn with_xthin(mut self, v: bool) -> Self {
		self.set_bit(4, v);
		self
		}

	fn set_bit(&mut self, bit: usize, bit_value: bool) {
		if bit_value {
			self.0 |= 1 << bit
		} else {
			self.0 &= !(1 << bit)
		}
	}

	fn bit_at(&self, bit: usize) -> bool {
		self.0 & (1 << bit) != 0
	}
}

impl Serializable for Services {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.0);
	}
}

impl Deserializable for Services {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		reader.read().map(Services)
	}
}
