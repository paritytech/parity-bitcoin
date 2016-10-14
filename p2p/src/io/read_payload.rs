use std::io;
use std::marker::PhantomData;
use futures::{Poll, Future};
use tokio_core::io::{read_exact, ReadExact};
use bytes::Bytes;
use hash::H32;
use crypto::checksum;
use message::{Error, MessageResult, Payload, deserialize_payload};

pub fn read_payload<M, A>(a: A, version: u32, len: usize, checksum: H32) -> ReadPayload<M, A>
	where A: io::Read, M: Payload {
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

impl<M, A> Future for ReadPayload<M, A> where A: io::Read, M: Payload {
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
