use std::sync::mpsc::{Sender, Receiver, channel};
use message::{Error, PayloadType};
use message::common::Command;
use message::types::{Addr, GetAddr};
use message::serialization::deserialize_payload;

struct Handler<S> {
	sender: Option<Sender<S>>,
}

impl<S> Default for Handler<S> {
	fn default() -> Self {
		Handler {
			sender: None,
		}
	}
}

impl<S> Handler<S> where S: PayloadType {
	fn command(&self) -> Command {
		S::command().into()
	}

	fn handle(&self, payload: &[u8], version: u32) -> Result<(), Error> {
		if let Some(ref sender) = self.sender {
			let payload: S = try!(deserialize_payload(payload, version));
			if let Err(_err) = sender.send(payload) {
				// TODO: unsubscribe channel on error?
				// TODO: trace!!!
			}
		}
		Ok(())
	}
}

#[derive(Default)]
pub struct Subscriber {
	addr: Handler<Addr>,
	getaddr: Handler<GetAddr>,
}

macro_rules! define_subscribe {
	($name: ident, $result: ident, $sub: ident) => {
		pub fn $name(&mut self) -> Receiver<$result> {
			let (sender, receiver) = channel();
			self.$sub.sender = Some(sender);
			receiver
		}
	}
}

macro_rules! maybe_handle {
	($command: expr, $sub: expr, $payload: expr, $version: expr) => {
		if $command == $sub.command() {
			return $sub.handle($payload, $version);
		}
	}
}

impl Subscriber {
	define_subscribe!(subscribe_addr, Addr, addr);
	define_subscribe!(subscribe_getaddr, GetAddr, getaddr);

	pub fn try_handle(&self, payload: &[u8], version: u32, command: Command) -> Result<(), Error> {
		maybe_handle!(command, self.addr, payload, version);
		maybe_handle!(command, self.getaddr, payload, version);
		Err(Error::InvalidCommand)
	}
}
