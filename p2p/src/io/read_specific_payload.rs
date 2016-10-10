use std::io;
use std::marker::PhantomData;
use futures::{Poll, Future};
use tokio_core::io::{read_exact, ReadExact};
use bytes::Bytes;
use hash::H32;
use ser::Deserializable;
use message::MessageResult;
use message::serialization::{PayloadType, deserialize_payload};

pub fn read_specific_payload<M, A>(a: A, version: u32, len: usize, checksum: H32) -> ReadSpecificPayload<M, A>
	where A: io::Read, M: PayloadType + Deserializable {
	ReadSpecificPayload {
		reader: read_exact(a, Bytes::new_with_len(len)),
		version: version,
		checksum: checksum,
		payload_type: PhantomData,
	}
}

pub struct ReadSpecificPayload<M, A> {
	reader: ReadExact<A, Bytes>,
	version: u32,
	checksum: H32,
	payload_type: PhantomData<M>,
}

/// TODO: check checksum
impl<M, A> Future for ReadSpecificPayload<M, A> where A: io::Read, M: PayloadType + Deserializable {
	type Item = (A, MessageResult<M>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (read, data) = try_ready!(self.reader.poll());
		let payload = deserialize_payload(&data, self.version);
		Ok((read, payload).into())
	}
}
