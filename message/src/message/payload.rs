use std::io;
use ser::{Reader, Stream};
use MessageResult;

pub trait Payload: Send + 'static {
 	fn version() -> u32;
	fn command() -> &'static str;
	fn deserialize_payload<T>(reader: &mut Reader<T>, version: u32) -> MessageResult<Self> where Self: Sized, T: io::Read;
	fn serialize_payload(&self, stream: &mut Stream, version: u32) -> MessageResult<()>;
}
