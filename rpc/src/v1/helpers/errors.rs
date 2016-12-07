///! RPC Error codes and error objects

macro_rules! rpc_unimplemented {
	() => (Err(::v1::helpers::errors::unimplemented(None)))
}

use std::fmt;
use jsonrpc_core::{Error, ErrorCode, Value};

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
