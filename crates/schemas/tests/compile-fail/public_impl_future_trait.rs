//! Test that invalid trait definitions fail to compile

// This should fail: trait with conflicting bounds
pub trait BadTrait {
    type Item: Clone + !Clone; // Conflicting bounds
}

fn main() {}