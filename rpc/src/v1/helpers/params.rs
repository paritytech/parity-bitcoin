///! Parameters parsing helpers

use jsonrpc_core::{Error, Params};
use v1::helpers::errors;

pub fn expect_no_params(params: Params) -> Result<(), Error> {
	match params {
		Params::None => Ok(()),
		p => Err(errors::invalid_params("No parameters were expected", p)),
	}
}
