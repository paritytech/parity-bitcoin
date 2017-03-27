use std::io;
use std::marker::PhantomData;
use futures::{Poll, Future};
use tokio_io::AsyncRead;
use tokio_io::io::{read_exact, ReadExact};
use bytes::Bytes;
use hash::H32;
use crypto::checksum;
use message::{Error, MessageResult, Payload, deserialize_payload};

pub fn read_payload<M, A>(a: A, version: u32, len: usize, checksum: H32) -> ReadPayload<M, A>
	where A: AsyncRead, M: Payload {
	ReadPayload {
		reader: read_exact(a, Bytes::new_with_len(len)),
		version: version,
		checksum: checksum,
		payload_type: PhantomData,
	}
}

pub struct ReadPayload<M, A> {
	reader: ReadExact<A, Bytes>,
	version: u32,
	checksum: H32,
	payload_type: PhantomData<M>,
}

impl<M, A> Future for ReadPayload<M, A> where A: AsyncRead, M: Payload {
	type Item = (A, MessageResult<M>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (read, data) = try_ready!(self.reader.poll());
		if checksum(&data) != self.checksum {
			return Ok((read, Err(Error::InvalidChecksum)).into());
		}
		let payload = deserialize_payload(&data, self.version);
		Ok((read, payload).into())
	}
}

#[cfg(test)]
mod tests {
	use futures::Future;
	use bytes::Bytes;
	use message::Error;
	use message::types::Ping;
	use super::read_payload;

	#[test]
	fn test_read_payload() {
		let raw: Bytes = "5845303b6da97786".into();
		let ping = Ping::new(u64::from_str_radix("8677a96d3b304558", 16).unwrap());
		assert_eq!(read_payload(raw.as_ref(), 0, 8, "83c00c76".into()).wait().unwrap().1, Ok(ping));
	}

	#[test]
	fn test_read_payload_with_invalid_checksum() {
		let raw: Bytes = "5845303b6da97786".into();
		assert_eq!(read_payload::<Ping, _>(raw.as_ref(), 0, 8, "83c00c75".into()).wait().unwrap().1, Err(Error::InvalidChecksum));
	}

	#[test]
	fn test_read_too_short_payload() {
		let raw: Bytes = "5845303b6da977".into();
		assert!(read_payload::<Ping, _>(raw.as_ref(), 0, 8, "83c00c76".into()).wait().is_err());
	}
}
