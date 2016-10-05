use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Read, Error};

pub struct ReadRc<A> {
	read: Rc<RefCell<A>>
}

impl<A> ReadRc<A> {
	pub fn new(a: Rc<RefCell<A>>) -> Self {
		ReadRc {
			read: a,
		}
	}
}

impl<A> From<A> for ReadRc<A> {
	fn from(a: A) -> Self {
		ReadRc::new(Rc::new(RefCell::new(a)))
	}
}

impl<A> Read for ReadRc<A> where A: Read {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
		self.read.borrow_mut().read(buf)
	}
}

impl<A> Clone for ReadRc<A> {
	fn clone(&self) -> Self {
		ReadRc::new(self.read.clone())
	}
}
