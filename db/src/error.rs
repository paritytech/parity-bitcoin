#[derive(Debug, PartialEq)]
pub enum Error {
	/// Low level database error
	DatabaseError(String),
	/// Invalid block
	CannotCanonize,
	/// Uknown parent
	UnknownParent,
	/// Ancient fork
	AncientFork,
}

impl From<Error> for String {
	fn from(e: Error) -> String {
		match e {
			Error::DatabaseError(s) => format!("Database error: {}", s),
			Error::CannotCanonize => "Cannot canonize block".into(),
			Error::UnknownParent => "Block parent is unknown".into(),
			Error::AncientFork => "Fork is too long to proceed".into(),
		}
	}
}
