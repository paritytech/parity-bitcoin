use {syn, quote};

pub fn impl_serializable(ast: &syn::DeriveInput) -> quote::Tokens {
	let body = match ast.body {
		syn::Body::Struct(ref s) => s,
		_ => panic!("#[derive(Serializable)] is only defined for structs."),
	};

	let stmts: Vec<_> = match *body {
		syn::VariantData::Struct(ref fields) => fields.iter().enumerate().map(serialize_field_map).collect(),
		syn::VariantData::Tuple(ref fields) => fields.iter().enumerate().map(serialize_field_map).collect(),
		syn::VariantData::Unit => panic!("#[derive(Serializable)] is not defined for Unit structs."),
	};

	let name = &ast.ident;

	let dummy_const = syn::Ident::new(format!("_IMPL_SERIALIZABLE_FOR_{}", name));
	let impl_block = quote! {
		impl serialization::Serializable for #name {
			fn serialize(&self, stream: &mut serialization::Stream) {
				#(#stmts)*
			}
		}
	};

	quote! {
		#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
		const #dummy_const: () = {
			extern crate serialization;
			#impl_block
		};
	}
}

fn serialize_field_map(tuple: (usize, &syn::Field)) -> quote::Tokens {
	serialize_field(tuple.0, tuple.1)
}

fn serialize_field(index: usize, field: &syn::Field) -> quote::Tokens {
	let ident = match field.ident {
		Some(ref ident) => ident.to_string(),
		None => index.to_string(),
	};

	let id = syn::Ident::new(format!("self.{}", ident));

	match field.ty {
		syn::Ty::Array(_, _) => quote! { stream.append_list(&#id); },
		syn::Ty::Slice(_) => quote! { stream.append_list(#id); },
		syn::Ty::Path(_, ref path) => {
			let ident = &path.segments.first().expect("there must be at least 1 segment").ident;
			match &ident.to_string() as &str {
				"Vec" => quote! { stream.append_list(&#id); },
				_ => quote! { stream.append(&#id); },
			}
		},
		_ => quote! { stream.append(&#id); },
	}
}
