use ser::{
	Serializable, Stream,
	Error as ReaderError, deserialize
};
use chain::{Transaction, Block};
use common::Command;
use types::{
	Version, Addr, AddrBelow31402, Inv,
	GetData, NotFound, GetBlocks, GetHeaders, Headers,
	Ping, Pong, Reject, FilterLoad, FilterAdd, FeeFilter,
	MerkleBlock, SendCompact, CompactBlock, GetBlockTxn, BlockTxn,
};

pub fn deserialize_payload(data: &[u8], version: u32, command: &Command) -> Result<Payload, ReaderError> {
	match &command.to_string() as &str {
		"version" => deserialize(data).map(Payload::Version),
		"verack" if data.is_empty() => Ok(Payload::Verack),
		"addr" => match version >= 31402 {
			true => deserialize(data).map(Payload::Addr),
			false => deserialize(data).map(Payload::AddrBelow31402),
		},
		"inv" => deserialize(data).map(Payload::Inv),
		"getdata" => deserialize(data).map(Payload::GetData),
		"notfound" => deserialize(data).map(Payload::NotFound),
		"getblocks" => deserialize(data).map(Payload::GetBlocks),
		"getheaders" => deserialize(data).map(Payload::GetHeaders),
		"tx" => deserialize(data).map(Payload::Tx),
		"block" => deserialize(data).map(Payload::Block),
		"headers" => deserialize(data).map(Payload::Headers),
		"getaddr" if data.is_empty() => Ok(Payload::GetAddr),
		"mempool" if data.is_empty() => Ok(Payload::MemPool),
		"ping" => deserialize(data).map(Payload::Ping),
		"pong" => deserialize(data).map(Payload::Pong),
		"reject" => deserialize(data).map(Payload::Reject),
		"filterload" => deserialize(data).map(Payload::FilterLoad),
		"filteradd" => deserialize(data).map(Payload::FilterAdd),
		"filterclear" if data.is_empty() => Ok(Payload::FilterClear),
		"merkleblock" => deserialize(data).map(Payload::MerkleBlock),
		"sendheaders" if data.is_empty() => Ok(Payload::SendHeaders),
		"feefilter" => deserialize(data).map(Payload::FeeFilter),
		"sendcmpct" => deserialize(data).map(Payload::SendCompact),
		"cmpctblock" => deserialize(data).map(Payload::CompactBlock),
		"getblocktxn" => deserialize(data).map(Payload::GetBlockTxn),
		"blocktxn" => deserialize(data).map(Payload::BlockTxn),
		_ => Err(ReaderError::MalformedData),
	}
}

#[derive(Debug, PartialEq)]
pub enum Payload {
	Version(Version),
	Verack,
	Addr(Addr),
	AddrBelow31402(AddrBelow31402),
	Inv(Inv),
	GetData(GetData),
	NotFound(NotFound),
	GetBlocks(GetBlocks),
	GetHeaders(GetHeaders),
	Tx(Transaction),
	Block(Block),
	Headers(Headers),
	GetAddr,
	MemPool,
	Ping(Ping),
	Pong(Pong),
	Reject(Reject),
	FilterLoad(FilterLoad),
	FilterAdd(FilterAdd),
	FilterClear,
	MerkleBlock(MerkleBlock),
	SendHeaders,
	FeeFilter(FeeFilter),
	SendCompact(SendCompact),
	CompactBlock(CompactBlock),
	GetBlockTxn(GetBlockTxn),
	BlockTxn(BlockTxn),
}

impl Payload {
	pub fn command(&self) -> Command {
		let cmd = match *self {
			Payload::Version(_) => "version",
			Payload::Verack => "verack",
			Payload::Addr(_) | Payload::AddrBelow31402(_) => "addr",
			Payload::Inv(_) => "inv",
			Payload::GetData(_) => "getdata",
			Payload::NotFound(_) => "notfound",
			Payload::GetBlocks(_) => "getblocks",
			Payload::GetHeaders(_) => "getheaders",
			Payload::Tx(_) => "tx",
			Payload::Block(_) => "block",
			Payload::Headers(_) => "headers",
			Payload::GetAddr => "getaddr",
			Payload::MemPool=> "mempool",
			Payload::Ping(_) => "ping",
			Payload::Pong(_) => "pong",
			Payload::Reject(_) => "reject",
			Payload::FilterLoad(_) => "filterload",
			Payload::FilterAdd(_) => "filteradd",
			Payload::FilterClear => "filterclear",
			Payload::MerkleBlock(_) => "merkleblock",
			Payload::SendHeaders => "sendheaders",
			Payload::FeeFilter(_) => "feefilter",
			Payload::SendCompact(_) => "sendcmpct",
			Payload::CompactBlock(_) => "compactblock",
			Payload::GetBlockTxn(_) => "getblocktxn",
			Payload::BlockTxn(_) => "blocktxn",
		};

		cmd.into()
	}

}

impl Serializable for Payload {
	fn serialize(&self, stream: &mut Stream) {
		match *self {
			Payload::Version(ref p) => { stream.append(p); },
			Payload::Verack => {},
			Payload::Addr(ref p) => { stream.append(p); },
			Payload::AddrBelow31402(ref p) => { stream.append(p); },
			Payload::Inv(ref p) => { stream.append(p); },
			Payload::GetData(ref p) => { stream.append(p); },
			Payload::NotFound(ref p) => { stream.append(p); },
			Payload::GetBlocks(ref p) => { stream.append(p); },
			Payload::GetHeaders(ref p) => { stream.append(p); },
			Payload::Tx(ref p) => { stream.append(p); },
			Payload::Block(ref p) => { stream.append(p); },
			Payload::Headers(ref p) => { stream.append(p); },
			Payload::GetAddr => {},
			Payload::MemPool => {},
			Payload::Ping(ref p) => { stream.append(p); },
			Payload::Pong(ref p) => { stream.append(p); },
			Payload::Reject(ref p) => { stream.append(p); },
			Payload::FilterLoad(ref p) => { stream.append(p); },
			Payload::FilterAdd(ref p) => { stream.append(p); },
			Payload::FilterClear => {},
			Payload::MerkleBlock(ref p) => { stream.append(p); },
			Payload::SendHeaders => {},
			Payload::FeeFilter(ref p) => { stream.append(p); },
			Payload::SendCompact(ref p) => { stream.append(p); },
			Payload::CompactBlock(ref p) => { stream.append(p); },
			Payload::GetBlockTxn(ref p) => { stream.append(p); },
			Payload::BlockTxn(ref p) => { stream.append(p); },
		}
	}
}
