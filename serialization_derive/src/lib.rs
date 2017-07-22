extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

mod ser;
mod de;

use proc_macro::TokenStream;
use ser::impl_raw_serialize;
use de::impl_raw_deserialize;

#[proc_macro_derive(RawSerialize)]
pub fn raw_serialize(input: TokenStream) -> TokenStream {
	let s = input.to_string();
	let ast = syn::parse_derive_input(&s).unwrap();
	let gen = impl_raw_serialize(&ast);
	gen.parse().unwrap()
}

#[proc_macro_derive(RawDeserialize)]
pub fn raw_deserialize(input: TokenStream) -> TokenStream {
	let s = input.to_string();
	let ast = syn::parse_derive_input(&s).unwrap();
	let gen = impl_raw_deserialize(&ast);
	gen.parse().unwrap()
}

