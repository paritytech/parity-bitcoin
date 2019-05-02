#[derive(Debug, PartialEq, Display)]
pub enum Error {
	/// Low level database error
	#[display(fmt = "Database error: {}", _0)]
	DatabaseError(String),
	/// Cannot canonize block
	#[display(fmt = "Cannot canonize block")]
	CannotCanonize,
	/// Cannot decanonize block
	#[display(fmt = "Cannot decanonize block")]
	CannotDecanonize,
	/// Uknown parent
	#[display(fmt = "Block parent is unknown")]
	UnknownParent,
	/// Ancient fork
	#[display(fmt = "Fork is too long to proceed")]
	AncientFork,
}

impl From<Error> for String {
	fn from(e: Error) -> String {
		format!("{}", e)
	}
}
