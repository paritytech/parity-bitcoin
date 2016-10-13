use std::sync::mpsc::{Sender, Receiver, channel};
use std::mem;
use parking_lot::Mutex;
use message::{Error, PayloadType};
use message::common::Command;
use message::types::{Addr, GetAddr};
use message::serialization::deserialize_payload;
use PeerId;

struct Handler<S> {
	sender: Mutex<Option<Sender<(S, PeerId)>>>,
}

impl<S> Default for Handler<S> {
	fn default() -> Self {
		Handler {
			sender: Mutex::default(),
		}
	}
}

impl<S> Handler<S> where S: PayloadType {
	fn command(&self) -> Command {
		S::command().into()
	}

	fn handle(&self, payload: &[u8], version: u32, peerid: PeerId) -> Result<(), Error> {
		let payload: S = try!(deserialize_payload(payload, version));
		if let Some(sender) = self.sender() {
			if let Err(_err) = sender.send((payload, peerid)) {
				// TODO: unsubscribe channel?
				// TODO: trace
			}
		}
		Ok(())
	}

	fn sender(&self) -> Option<Sender<(S, PeerId)>> {
		self.sender.lock().clone()
	}

	fn store(&self, sender: Sender<(S, PeerId)>) {
		mem::replace(&mut *self.sender.lock(), Some(sender));
	}
}

#[derive(Default)]
pub struct Subscriber {
	addr: Handler<Addr>,
	getaddr: Handler<GetAddr>,
}

macro_rules! define_subscribe {
	($name: ident, $result: ident, $sub: ident) => {
		pub fn $name(&self) -> Receiver<($result, PeerId)> {
			let (sender, receiver) = channel();
			self.$sub.store(sender);
			receiver
		}
	}
}

macro_rules! maybe_handle {
	($command: expr, $sub: expr, $payload: expr, $version: expr, $peerid: expr) => {
		if $command == $sub.command() {
			return $sub.handle($payload, $version, $peerid);
		}
	}
}

impl Subscriber {
	define_subscribe!(subscribe_addr, Addr, addr);
	define_subscribe!(subscribe_getaddr, GetAddr, getaddr);

	pub fn try_handle(&self, payload: &[u8], version: u32, command: Command, peerid: PeerId) -> Result<(), Error> {
		maybe_handle!(command, self.addr, payload, version, peerid);
		maybe_handle!(command, self.getaddr, payload, version, peerid);
		Err(Error::InvalidCommand)
	}
}
