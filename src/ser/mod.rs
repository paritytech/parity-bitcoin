mod compact_integer;
pub mod reader;
pub mod stream;

pub use self::reader::{Reader, Deserializable, deserialize, Error};
pub use self::stream::{Stream, Serializable, serialize};
