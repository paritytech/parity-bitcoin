mod ping;

use message::{Command, Error};
use PeerId;

pub trait Protocol {
	fn start(&self);
	fn try_handle(&self, command: Command, paylaod: &[u8], version: u32, peerid: PeerId) -> Result<(), Error>;
}
