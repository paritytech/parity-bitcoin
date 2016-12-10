///! RPC Error codes and error objects

mod codes {
	// NOTE [ToDr] Codes from [-32099, -32000]
	pub const EXECUTION_ERROR: i64 = -32015;
	pub const BLOCK_NOT_FOUND: i64 = -32099;
}

use std::fmt;
use jsonrpc_core::{Error, ErrorCode, Value};

macro_rules! rpc_unimplemented {		
	() => (Err(::v1::helpers::errors::unimplemented(None)))		
}

pub fn unimplemented(details: Option<String>) -> Error {		
	Error {		
		code: ErrorCode::InternalError,		
		message: "This request is not implemented yet. Please create an issue on Github repo.".into(),		
		data: details.map(Value::String),		
	}		
}

pub fn invalid_params<T: fmt::Debug>(param: &str, details: T) -> Error {
	Error {
		code: ErrorCode::InvalidParams,
		message: format!("Couldn't parse parameters: {}", param),
		data: Some(Value::String(format!("{:?}", details))),
	}
}

pub fn execution<T: fmt::Debug>(data: T) -> Error {
	Error {
		code: ErrorCode::ServerError(codes::EXECUTION_ERROR),
		message: "Execution error.".into(),
		data: Some(Value::String(format!("{:?}", data))),
	}
}

pub fn block_not_found<T: fmt::Debug>(data: T) -> Error {
	Error {
		code: ErrorCode::ServerError(codes::BLOCK_NOT_FOUND),
		message: "Block with given hash is not found".into(),
		data: Some(Value::String(format!("{:?}", data))),
	}
}

pub fn block_at_height_not_found<T: fmt::Debug>(data: T) -> Error {
	Error {
		code: ErrorCode::ServerError(codes::BLOCK_NOT_FOUND),
		message: "Block at given height is not found".into(),
		data: Some(Value::String(format!("{:?}", data))),
	}
}
