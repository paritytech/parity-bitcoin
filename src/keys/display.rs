use std::ops::Deref;
use keys::Error;

pub trait DisplayLayout {
	type Target: Deref<Target = [u8]>;

	fn layout(&self) -> Self::Target;

	fn from_layout(data: &[u8]) -> Result<Self, Error> where Self: Sized;
}
