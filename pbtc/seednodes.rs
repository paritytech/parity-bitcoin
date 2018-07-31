
pub fn mainnet_seednodes() -> Vec<&'static str> {
	vec![
		// Pieter Wuille
		"seed.bitcoin.sipa.be:8333",
		// Matt Corallo
		"dnsseed.bluematt.me:8333",
		// Luke Dashjr
		"dnsseed.bitcoin.dashjr.org:8333",
		// Christian Decker
		"seed.bitcoinstats.com:8333",
		// Jonas Schnelli
		"seed.bitcoin.jonasschnelli.ch:8333",
		// Peter Todd
		"seed.btc.petertodd.org:8333",
		//
		"seed.voskuil.org:8333",
	]
}

pub fn testnet_seednodes() -> Vec<&'static str> {
	vec![
		"testnet-seed.bitcoin.jonasschnelli.ch:18333",
		"seed.tbtc.petertodd.org:18333",
		"testnet-seed.bluematt.me:18333",
		"testnet-seed.bitcoin.schildbach.de:18333",
		"testnet-seed.voskuil.org:18333",
	]
}

pub fn bitcoin_cash_seednodes() -> Vec<&'static str> {
	vec![
		"seed.bitcoinabc.org:8333",
		"seed-abc.bitcoinforks.org:8333",
		"seed.bitprim.org:8333",
		"seed.deadalnix.me:8333",
		"seeder.criptolayer.net:8333"
	]
}

pub fn bitcoin_cash_testnet_seednodes() -> Vec<&'static str> {
	vec![
		"testnet-seed.bitcoinabc.org:18333",
		"testnet-seed-abc.bitcoinforks.org:18333",
		"testnet-seed.bitprim.org:18333",
		"testnet-seed.deadalnix.me:18333",
		"testnet-seeder.criptolayer.net:18333"
	]
}
