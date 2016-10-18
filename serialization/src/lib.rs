extern crate byteorder;
extern crate primitives;

mod compact_integer;
mod impls;
pub mod reader;
pub mod stream;

pub use primitives::{hash, bytes};

pub use compact_integer::CompactInteger;
pub use self::reader::{Reader, Deserializable, deserialize, Error};
pub use self::stream::{Stream, Serializable, serialize, serialized_list_size};
