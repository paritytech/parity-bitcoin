use std::sync::Arc;
use parking_lot::Mutex;
use message::{Payload, serialize_payload, Message};
use p2p::Context;
use util::{PeerInfo, ConfigurableSynchronizer, ResponseQueue, Synchronizer, Responses};

pub struct PeerContext {
	context: Arc<Context>,
	info: PeerInfo,
	synchronizer: Mutex<ConfigurableSynchronizer>,
	response_queue: Mutex<ResponseQueue>,
}

impl PeerContext {
	pub fn new(context: Arc<Context>, info: PeerInfo, synchronous: bool) -> Self {
		PeerContext {
			context: context,
			info: info,
			synchronizer: Mutex::new(ConfigurableSynchronizer::new(synchronous)),
			response_queue: Mutex::default(),
		}
	}

	fn to_message<T>(&self, payload: &T) -> Message<T> where T: Payload {
		Message::new(self.info.magic, self.info.version, payload).expect("failed to create outgoing message")
	}

	fn send_awaiting(&self, sync: &mut ConfigurableSynchronizer, queue: &mut ResponseQueue, start_id: u32) {
		let mut next_id = start_id;
		loop {
			next_id = next_id.overflowing_add(1).0;
			match queue.responses(next_id) {
				Some(Responses::Finished(messages)) => {
					assert!(sync.permission_for_response(next_id));
					for message in messages {
						let send = Context::send_message_to_peer(self.context.clone(), self.info.id, message);
						self.context.spawn(send);
					}
				},
				Some(Responses::Unfinished(messages)) => {
					assert!(sync.is_permitted(next_id));
					for message in messages {
						let send = Context::send_message_to_peer(self.context.clone(), self.info.id, message);
						self.context.spawn(send);
					}
					break;
				},
				None => {
					break;
				}
			}
		}
	}

	/// Request is always automatically send.
	pub fn send_request<T>(&self, payload: &T) where T: Payload {
		let send = Context::send_to_peer(self.context.clone(), self.info.id, payload);
		self.context.spawn(send);
	}

	pub fn declare_response(&self) -> u32 {
		self.synchronizer.lock().declare_response()
	}

	pub fn send_response_inline<T>(&self, payload: &T) where T: Payload {
		let id = self.declare_response();
		self.send_response(payload, id, true);
	}

	/// Responses are sent in order defined by synchronizer.
	pub fn send_response<T>(&self, payload: &T, id: u32, is_final: bool) where T: Payload {
		let mut sync = self.synchronizer.lock();
		let mut queue = self.response_queue.lock();
		if is_final {
			if sync.permission_for_response(id) {
				let send = Context::send_to_peer(self.context.clone(), self.info.id, payload);
				self.context.spawn(send);
				self.send_awaiting(&mut sync, &mut queue, id);
			} else {
				queue.push_finished_response(id, self.to_message(payload).into());
			}
		} else {
			if sync.is_permitted(id) {
				let send = Context::send_to_peer(self.context.clone(), self.info.id, payload);
				self.context.spawn(send);
			} else {
				queue.push_unfinished_response(id, self.to_message(payload).into());
			}
		}
	}

	pub fn info(&self) -> &PeerInfo {
		&self.info
	}

	pub fn global(&self) -> &Arc<Context> {
		&self.context
	}
}
