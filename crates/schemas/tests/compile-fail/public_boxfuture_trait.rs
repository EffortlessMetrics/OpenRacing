//! Test that invalid syntax fails to compile

// This should fail: invalid syntax
pub trait BadSyntax {
    fn method(&self) -> Result<String, >; // Missing error type
}

fn main() {}