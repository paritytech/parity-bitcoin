use std::collections::HashMap;
use bytes::Bytes;

/// Queue of out-of-order responses. Each peer has it's own queue.
#[derive(Debug, Default)]
pub struct ResponseQueue {
	unfinished: HashMap<u32, Vec<Bytes>>,
	finished: HashMap<u32, Vec<Bytes>>,
}

pub enum Responses {
	Unfinished(Vec<Bytes>),
	Finished(Vec<Bytes>),
}

impl ResponseQueue {
	pub fn push_unfinished_response(&mut self, id: u32, response: Bytes) {
		self.unfinished.entry(id).or_insert_with(Vec::new).push(response)
	}

	pub fn push_finished_response(&mut self, id: u32, response: Bytes) {
		let mut responses = self.unfinished.remove(&id).unwrap_or_default();
		responses.push(response);
		let previous = self.finished.insert(id, responses);
		assert!(previous.is_none(), "logic error; same finished response should never be pushed twice");
	}

	pub fn responses(&mut self, id: u32) -> Option<Responses> {
		self.unfinished.remove(&id).map(Responses::Unfinished)
			.or_else(|| self.finished.remove(&id).map(Responses::Finished))
	}
}
