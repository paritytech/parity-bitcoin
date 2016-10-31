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

pub struct StaticNonce(u64);

impl StaticNonce {
	pub fn new(nonce: u64) -> Self {
		StaticNonce(nonce)
	}
}

impl NonceGenerator for StaticNonce {
	fn get(&self) -> u64 {
		self.0
	}
}
