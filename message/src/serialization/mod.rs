mod stream;
mod reader;

pub use self::stream::{PayloadStream, serialize_payload};
pub use self::reader::{PayloadReader, deserialize_payload};
use ser::{Reader, Stream};
use MessageResult;

pub trait PayloadType: Send + 'static {
 	fn version() -> u32;
	fn command() -> &'static str;
	fn deserialize_payload(reader: &mut Reader, version: u32) -> MessageResult<Self> where Self: Sized;
	fn serialize_payload(&self, stream: &mut Stream, version: u32) -> MessageResult<()>;
}
