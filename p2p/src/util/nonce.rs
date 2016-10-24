use rand;

pub trait NonceGenerator {
	fn get(&self) -> u64;
}

#[derive(Default)]
pub struct RandomNonce;

impl NonceGenerator for RandomNonce {
	fn get(&self) -> u64 {
		rand::random()
	}
}

pub struct _StaticNonce(u64);

impl _StaticNonce {
	pub fn _new(nonce: u64) -> Self {
		_StaticNonce(nonce)
	}
}

impl NonceGenerator for _StaticNonce {
	fn get(&self) -> u64 {
		self.0
	}
}
