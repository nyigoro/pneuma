#[derive(Debug, Default)]
pub struct LadybirdBridge;

impl LadybirdBridge {
    pub fn api_version(&self) -> u32 {
        1
    }
}
