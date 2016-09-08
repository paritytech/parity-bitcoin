use script::{Opcode, Script};

pub struct Builder {
	data: Vec<u8>,
}

impl Builder {
	pub fn new() -> Self {
		Builder {
			data: Vec::new(),
		}
	}

	pub fn push_opcode(&mut self, opcode: Opcode) -> &mut Self {
		self.data.push(opcode as u8);
		self
	}

	pub fn into_script(self) -> Script {
		Script::new(self.data)
	}
}
