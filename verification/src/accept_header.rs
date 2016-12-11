use db::IndexedBlockHeader;
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

#[derive(Clone, Copy)]
pub struct CanonHeader<'a> {
	header: &'a IndexedBlockHeader,
}

impl<'a> CanonHeader<'a> {
	pub fn new(header: &'a IndexedBlockHeader) -> Self {
		CanonHeader {
			header: header,
		}
	}
}
