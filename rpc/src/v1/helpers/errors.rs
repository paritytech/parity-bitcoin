///! RPC Error codes and error objects

mod codes {
	// NOTE [ToDr] Codes from [-32099, -32000]
	pub const EXECUTION_ERROR: i64 = -32015;
}

use std::fmt;
use jsonrpc_core::{Error, ErrorCode, Value};

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

