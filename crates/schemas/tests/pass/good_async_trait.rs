//! Test that proper async trait patterns compile successfully

use async_trait::async_trait;

// This should compile fine - using async_trait is the correct pattern
#[async_trait]
pub trait GoodAsyncTrait {
    async fn do_work(&self) -> String;
    async fn process_data(&self, data: &[u8]) -> Result<Vec<u8>, String>;
}

// Implementation should also work
struct MyImpl;

#[async_trait]
impl GoodAsyncTrait for MyImpl {
    async fn do_work(&self) -> String {
        "work done".to_string()
    }
    
    async fn process_data(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(data.to_vec())
    }
}

fn main() {}