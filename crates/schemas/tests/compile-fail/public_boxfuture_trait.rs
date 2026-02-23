//! Test that invalid syntax fails to compile

// This should fail: unresolved error type in return signature
pub trait BadSyntax {
    fn method(&self) -> Result<String, crate::__MissingErrorType>;
}

fn main() {}
