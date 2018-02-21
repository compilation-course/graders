use std::result::Result;

#[derive(Deserialize)]
pub struct AMQPConfiguration {
    host: String,
    port: u16,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AMQPRequest<O> {
    step: String,
    zip_url: String,
    result_queue: String,
    opaque: O,
}

pub fn enqueue<O>(result_queue: &str, step: &str, zip_url: &str) -> Result<(), ()> {
    Ok(())
}
