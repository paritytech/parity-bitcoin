use std::io;
use {Serializable, Deserializable, Error, Reader, Stream};

#[derive(Debug, Clone)]
pub struct List<T>(Vec<T>);

impl<T> List<T> where T: Serializable + Deserializable {
	pub fn from(vec: Vec<T>) -> Self {
		List(vec)
	}

	pub fn into(self) -> Vec<T> {
		self.0
	}
}

impl<S> Serializable for List<S> where S: Serializable {
	fn serialize(&self, s: &mut Stream) {
		s.append_list(&self.0);
	}
}

impl<D> Deserializable for List<D> where D: Deserializable {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error> where T: io::Read {
		reader.read_list().map(List)
	}
}
