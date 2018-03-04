use std::ops;
use Error;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Stack<T> {
	data: Vec<T>,
}

impl<T> From<Vec<T>> for Stack<T> {
	fn from(v: Vec<T>) -> Self {
		Stack {
			data: v
		}
	}
}

impl<T> ops::Deref for Stack<T> {
	type Target = Vec<T>;

	fn deref(&self) -> &Self::Target {
		&self.data
	}
}

impl<T> ops::DerefMut for Stack<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.data
	}
}

impl<T> Stack<T> {
	#[inline]
	pub fn new() -> Self {
		Stack {
			data: Vec::new()
		}
	}

	#[inline]
	pub fn require(&self, len: usize) -> Result<(), Error> {
		if self.data.len() < len {
			return Err(Error::InvalidStackOperation);
		}
		Ok(())
	}

	#[inline]
	pub fn last(&self) -> Result<&T, Error> {
		self.data.last().ok_or(Error::InvalidStackOperation)
	}

	#[inline]
	pub fn last_mut(&mut self) -> Result<&mut T, Error> {
		self.data.last_mut().ok_or(Error::InvalidStackOperation)
	}

	#[inline]
	pub fn pop(&mut self) -> Result<T, Error> {
		self.data.pop().ok_or(Error::InvalidStackOperation)
	}

	#[inline]
	pub fn push(&mut self, value: T) {
		self.data.push(value)
	}

	#[inline]
	pub fn top(&self, i: usize) -> Result<&T, Error> {
		let pos = i + 1;
		self.require(pos)?;
		Ok(&self.data[self.data.len() - pos])
	}

	#[inline]
	pub fn remove(&mut self, i: usize) -> Result<T, Error> {
		let pos = i + 1;
		self.require(pos)?;
		let to_remove = self.data.len() - pos;
		Ok(self.data.remove(to_remove))
	}

	pub fn drop(&mut self, i: usize) -> Result<(), Error> {
		self.require(i)?;
		let mut j = i;
		while j > 0 {
			self.data.pop();
			j -= 1;
		}
		Ok(())
	}

	pub fn dup(&mut self, i: usize) -> Result<(), Error> where T: Clone {
		self.require(i)?;
		let mut j = i;
		while j > 0 {
			let v = self.data[self.data.len() - i].clone();
			self.data.push(v);
			j -= 1;
		}
		Ok(())
	}

	pub fn over(&mut self, i: usize) -> Result<(), Error> where T: Clone {
		let mut j = i * 2;
		self.require(j)?;
		let to_clone = j;
		while j > i {
			let v = self.data[self.data.len() - to_clone].clone();
			self.data.push(v);
			j -= 1;
		}
		Ok(())
	}

	pub fn rot(&mut self, i: usize) -> Result<(), Error> {
		let mut j = i * 3;
		self.require(j)?;
		let to_remove = self.data.len() - j;
		let limit = j - i;
		while j > limit {
			let v = self.data.remove(to_remove);
			self.data.push(v);
			j -= 1;
		}
		Ok(())
	}

	pub fn swap(&mut self, i: usize) -> Result<(), Error> {
		let mut j = i * 2;
		let mut k = i;
		self.require(j)?;
		let len = self.data.len();
		while k > 0 {
			self.data.swap(len - j, len - k);
			j -= 1;
			k -= 1;
		}
		Ok(())
	}

	pub fn nip(&mut self) -> Result<(), Error> {
		self.require(2)?;
		let len = self.data.len();
		self.data.swap_remove(len - 2);
		Ok(())
	}

	pub fn tuck(&mut self) -> Result<(), Error> where T: Clone {
		self.require(2)?;
		let len = self.data.len();
		let v = self.data[len - 1].clone();
		self.data.insert(len - 2, v);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use Error;
	use super::Stack;

	#[test]
	fn test_stack_require() {
		let stack: Stack<u8> = vec![].into();
		assert_eq!(stack.require(0), Ok(()));
		assert_eq!(stack.require(1), Err(Error::InvalidStackOperation));
		let stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.require(0), Ok(()));
		assert_eq!(stack.require(1), Ok(()));
		assert_eq!(stack.require(2), Err(Error::InvalidStackOperation));
		let stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.require(0), Ok(()));
		assert_eq!(stack.require(1), Ok(()));
		assert_eq!(stack.require(2), Ok(()));
		assert_eq!(stack.require(3), Err(Error::InvalidStackOperation));
	}

	#[test]
	fn test_stack_last() {
		let stack: Stack<u8> = vec![].into();
		assert_eq!(stack.last(), Err(Error::InvalidStackOperation));
		let stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.last(), Ok(&0));
		let stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.last(), Ok(&5));
	}

	#[test]
	fn test_stack_pop() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.pop(), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![].into());
		let mut stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.pop(), Ok(0));
		assert_eq!(stack, vec![].into());
		let mut stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.pop(), Ok(5));
		assert_eq!(stack.pop(), Ok(0));
		assert_eq!(stack, vec![].into());
	}

	#[test]
	fn test_stack_push() {
		let mut stack: Stack<u8> = vec![].into();
		stack.push(0);
		assert_eq!(stack, vec![0].into());
		stack.push(5);
		assert_eq!(stack, vec![0, 5].into());
	}

	#[test]
	fn test_stack_top() {
		let stack: Stack<u8> = vec![].into();
		assert_eq!(stack.top(0), Err(Error::InvalidStackOperation));
		let stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.top(0), Ok(&0));
		assert_eq!(stack.top(1), Err(Error::InvalidStackOperation));
		let stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.top(0), Ok(&5));
		assert_eq!(stack.top(1), Ok(&0));
		assert_eq!(stack.top(2), Err(Error::InvalidStackOperation));
	}

	#[test]
	fn test_stack_remove() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.remove(0), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![].into());
		let mut stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.remove(0), Ok(0));
		assert_eq!(stack.remove(0), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![].into());
		let mut stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.remove(1), Ok(0));
		assert_eq!(stack, vec![5].into());
		assert_eq!(stack.remove(0), Ok(5));
		assert_eq!(stack, vec![].into());
	}

	#[test]
	fn test_stack_drop() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.drop(0), Ok(()));
		let mut stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.drop(0), Ok(()));
		assert_eq!(stack, vec![0, 5].into());
		assert_eq!(stack.drop(3), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![0, 5].into());
		assert_eq!(stack.drop(1), Ok(()));
		assert_eq!(stack, vec![0].into());
		let mut stack: Stack<u8> = vec![3, 5, 0].into();
		assert_eq!(stack.drop(3), Ok(()));
		assert_eq!(stack, vec![].into());
	}

	#[test]
	fn test_stack_dup() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.dup(0), Ok(()));
		assert_eq!(stack.dup(1), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![].into());
		let mut stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.dup(0), Ok(()));
		assert_eq!(stack.dup(2), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![0].into());
		assert_eq!(stack.dup(1), Ok(()));
		assert_eq!(stack, vec![0, 0].into());
		assert_eq!(stack.dup(2), Ok(()));
		assert_eq!(stack, vec![0, 0, 0, 0].into());
		let mut stack: Stack<u8> = vec![0, 1].into();
		assert_eq!(stack.dup(2), Ok(()));
		assert_eq!(stack, vec![0, 1, 0, 1].into());
	}

	#[test]
	fn test_stack_over() {
		let mut stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.over(0), Ok(()));
		assert_eq!(stack.over(1), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.over(2), Err(Error::InvalidStackOperation));
		assert_eq!(stack.over(1), Ok(()));
		assert_eq!(stack, vec![0, 5, 0].into());
		assert_eq!(stack.over(1), Ok(()));
		assert_eq!(stack, vec![0, 5, 0, 5].into());
		let mut stack: Stack<u8> = vec![1, 2, 3, 4].into();
		assert_eq!(stack.over(2), Ok(()));
		assert_eq!(stack, vec![1, 2, 3, 4, 1, 2].into());
	}

	#[test]
	fn test_stack_rot() {
		let mut stack: Stack<u8> = vec![0, 5].into();
		assert_eq!(stack.rot(0), Ok(()));
		assert_eq!(stack.rot(1), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![0, 5].into());
		let mut stack: Stack<u8> = vec![0, 1, 2].into();
		assert_eq!(stack.rot(2), Err(Error::InvalidStackOperation));
		assert_eq!(stack.rot(1), Ok(()));
		assert_eq!(stack, vec![1, 2, 0].into());
		let mut stack: Stack<u8> = vec![0, 1, 2, 3].into();
		assert_eq!(stack.rot(1), Ok(()));
		assert_eq!(stack, vec![0, 2, 3, 1].into());
		let mut stack: Stack<u8> = vec![0, 1, 2, 3, 4, 5].into();
		assert_eq!(stack.rot(3), Err(Error::InvalidStackOperation));
		assert_eq!(stack.rot(2), Ok(()));
		assert_eq!(stack, vec![2, 3, 4, 5, 0, 1].into());
	}

	#[test]
	fn test_stack_swap() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.swap(0), Ok(()));
		assert_eq!(stack, vec![].into());
		assert_eq!(stack.swap(1), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0, 1, 2, 3].into();
		assert_eq!(stack.swap(0), Ok(()));
		assert_eq!(stack, vec![0, 1, 2, 3].into());
		assert_eq!(stack.swap(1), Ok(()));
		assert_eq!(stack, vec![0, 1, 3, 2].into());
		assert_eq!(stack.swap(2), Ok(()));
		assert_eq!(stack, vec![3, 2, 0, 1].into());
		assert_eq!(stack.swap(3), Err(Error::InvalidStackOperation));
		assert_eq!(stack, vec![3, 2, 0, 1].into());
	}

	#[test]
	fn test_stack_nip() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.nip(), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.nip(), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0, 1].into();
		assert_eq!(stack.nip(), Ok(()));
		assert_eq!(stack, vec![1].into());
		assert_eq!(stack.nip(), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0, 1, 2, 3].into();
		assert_eq!(stack.nip(), Ok(()));
		assert_eq!(stack, vec![0, 1, 3].into());
		assert_eq!(stack.nip(), Ok(()));
		assert_eq!(stack, vec![0, 3].into());
	}

	#[test]
	fn test_stack_tuck() {
		let mut stack: Stack<u8> = vec![].into();
		assert_eq!(stack.tuck(), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0].into();
		assert_eq!(stack.tuck(), Err(Error::InvalidStackOperation));
		let mut stack: Stack<u8> = vec![0, 1].into();
		assert_eq!(stack.tuck(), Ok(()));
		assert_eq!(stack, vec![1, 0, 1].into());
		assert_eq!(stack.tuck(), Ok(()));
		assert_eq!(stack, vec![1, 1, 0, 1].into());
		let mut stack: Stack<u8> = vec![0, 1, 2, 3].into();
		assert_eq!(stack.tuck(), Ok(()));
		assert_eq!(stack, vec![0, 1, 3, 2, 3].into());
		assert_eq!(stack.tuck(), Ok(()));
		assert_eq!(stack, vec![0, 1, 3, 3, 2, 3].into());
	}
}
