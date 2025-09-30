//! Test that proper async trait patterns compile successfully

use async_trait::async_trait;

// This should compile successfully: proper async trait usage
#[async_trait]
pub trait GoodAsyncApi {
    async fn do_async_work(&self) -> Result<String, Box<dyn std::error::Error>>;
}

struct Implementation;

#[async_trait]
impl GoodAsyncApi for Implementation {
    async fn do_async_work(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok("success".to_string())
    }
}

fn main() {}