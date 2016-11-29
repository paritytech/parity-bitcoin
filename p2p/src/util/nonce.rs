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
