pub mod chrome_120;
pub mod firefox_121;

#[derive(Debug, Clone, Copy)]
pub struct BrowserProfile {
    pub id: &'static str,
    pub user_agent: &'static str,
    pub platform: &'static str,
}
