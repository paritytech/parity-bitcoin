use canon::CanonHeader;
use error::Error;

pub struct HeaderAcceptor<'a> {
	_tmp: CanonHeader<'a>,
}

impl<'a> HeaderAcceptor<'a> {
	pub fn new(header: CanonHeader<'a>) -> Self {
		HeaderAcceptor {
			_tmp: header,
		}
	}

	pub fn check(&self) -> Result<(), Error> {
		Ok(())
	}
}
