
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

pub fn segwit2x_seednodes() -> Vec<&'static str> {
	vec![
		"seed.mainnet.b-pay.net:8333",
		"seed.ob1.io:8333",
		"seed.blockchain.info:8333",
		"bitcoin.bloqseeds.net:8333",
	]
}
