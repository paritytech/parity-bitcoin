use std::collections::{HashMap, HashSet};
use bytes::Bytes;

/// Queue of out-of-order responses. Each peer has it's own queue.
#[derive(Debug, Default)]
pub struct ResponseQueue {
	unfinished: HashMap<u32, Vec<Bytes>>,
	finished: HashMap<u32, Vec<Bytes>>,
	ignored: HashSet<u32>,
}

pub enum Responses {
	Unfinished(Vec<Bytes>),
	Finished(Vec<Bytes>),
	Ignored,
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

	pub fn push_ignored_response(&mut self, id: u32) {
		assert!(self.ignored.insert(id), "logic error; same response should never be ignored twice");
	}

	pub fn responses(&mut self, id: u32) -> Option<Responses> {
		self.unfinished.remove(&id).map(Responses::Unfinished)
			.or_else(|| self.finished.remove(&id).map(Responses::Finished))
			.or_else(|| {
				if self.ignored.remove(&id) {
					Some(Responses::Ignored)
				} else {
					None
				}
			})
	}
}
