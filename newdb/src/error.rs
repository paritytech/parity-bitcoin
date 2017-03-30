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
