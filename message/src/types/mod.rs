mod addr;
mod getblocks;
mod headers;
mod inv;
pub mod version;

pub use self::addr::{Addr, AddrBelow31402};
pub use self::getblocks::GetBlocks;
pub use self::headers::Headers;
pub use self::inv::Inv;
pub use self::version::Version;

pub type GetData = Inv;
pub type NotFound = Inv;
pub type GetHeaders = GetBlocks;
