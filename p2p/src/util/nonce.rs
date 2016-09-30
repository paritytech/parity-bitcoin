use rand;

pub trait NonceGenerator {
	fn get(&self) -> u32;
}

#[derive(Default)]
pub struct RandomNonce;

impl NonceGenerator for RandomNonce {
	fn get(&self) -> u32 {
		rand::random()
	}
}

pub struct StaticNonce(u32);

impl StaticNonce {
	pub fn new(nonce: u32) -> Self {
		StaticNonce(nonce)
	}
}

impl NonceGenerator for StaticNonce {
	fn get(&self) -> u32 {
		self.0
	}
}
