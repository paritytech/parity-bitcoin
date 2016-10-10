mod stream;
mod reader;

use ser::{Reader, Deserializable};
use {MessageResult, Error};
pub use self::stream::PayloadStream;
pub use self::reader::{PayloadReader, deserialize_payload};

pub trait PayloadType: Deserializable {
 	fn version() -> u32;
	fn command() -> &'static str;
	fn deserialize_payload(reader: &mut Reader, version: u32) -> MessageResult<Self> where Self: Sized {
		if version < Self::version() {
			return Err(Error::InvalidVersion);
		}

		Self::deserialize(reader).map_err(Into::into)
	}
}
