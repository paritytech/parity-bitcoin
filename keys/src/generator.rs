use rand::os::OsRng;
use network::Network;
use {KeyPair, SECP256K1, Error};

pub trait Generator {
	fn generate(&self) -> Result<KeyPair, Error>;
}

pub struct Random {
	network: Network
}

impl Random {
	pub fn new(network: Network) -> Self {
		Random {
			network: network,
		}
	}
}

impl Generator for Random {
	fn generate(&self) -> Result<KeyPair, Error> {
		let context = &SECP256K1;
		let mut rng = try!(OsRng::new().map_err(|_| Error::FailedKeyGeneration));
		let (secret, public) = try!(context.generate_keypair(&mut rng));
		Ok(KeyPair::from_keypair(secret, public, self.network))
	}
}
