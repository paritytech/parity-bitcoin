use std::io;
use {Deserializable, Error, Reader};

pub struct DeserializableList<T>(Vec<T>) where T: Deserializable;

impl<T> DeserializableList<T> where T: Deserializable{
	pub fn into(self) -> Vec<T> {
		self.0
	}
}

impl<D> Deserializable for DeserializableList<D> where D: Deserializable {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error> where T: io::Read {
		reader.read_list().map(DeserializableList)
	}
}
