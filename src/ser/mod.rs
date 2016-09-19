mod compact_integer;
mod impls;
pub mod reader;
pub mod stream;

pub use self::reader::{Reader, Deserializable, deserialize, Error};
pub use self::stream::{Stream, Serializable, serialize};
