use std::ops::Deref;

pub trait DisplayLayout {
	type Target: Deref<Target = [u8]>;

	fn layout(&self) -> Self::Target;
}
