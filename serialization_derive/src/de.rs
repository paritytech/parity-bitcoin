use {syn, quote};

pub fn impl_deserializable(ast: &syn::DeriveInput) -> quote::Tokens {
	let body = match ast.body {
		syn::Body::Struct(ref s) => s,
		_ => panic!("#[derive(Deserializable)] is only defined for structs."),
	};

	let stmts: Vec<_> = match *body {
		syn::VariantData::Struct(ref fields) => fields.iter().enumerate().map(deserialize_field_map).collect(),
		syn::VariantData::Tuple(ref fields) => fields.iter().enumerate().map(deserialize_field_map).collect(),
		syn::VariantData::Unit => panic!("#[derive(Deserializable)] is not defined for Unit structs."),
	};

	let name = &ast.ident;

	let dummy_const = syn::Ident::new(format!("_IMPL_DESERIALIZABLE_FOR_{}", name));
	let impl_block = quote! {
		impl serialization::Deserializable for #name {
			fn deserialize<T>(reader: &mut serialization::Reader<T>) -> Result<Self, serialization::Error> where T: io::Read {
				let result = #name {
					#(#stmts)*
				};

				Ok(result)
			}
		}
	};

	quote! {
		#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
		const #dummy_const: () = {
			extern crate serialization;
			use std::io;
			#impl_block
		};
	}
}

fn deserialize_field_map(tuple: (usize, &syn::Field)) -> quote::Tokens {
	deserialize_field(tuple.0, tuple.1)
}

fn deserialize_field(index: usize, field: &syn::Field) -> quote::Tokens {
	let ident = match field.ident {
		Some(ref ident) => ident.to_string(),
		None => index.to_string(),
	};

	let id = syn::Ident::new(ident.to_string());

	match field.ty {
		syn::Ty::Path(_, ref path) => {
			let ident = &path.segments.first().expect("there must be at least 1 segment").ident;
			if &ident.to_string() == "Vec" {
				quote! { #id: reader.read_list()?, }
			} else {
				quote! { #id: reader.read()?, }
			}
		},
		_ => panic!("serialization not supported"),
	}
}
