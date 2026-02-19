use super::BrowserProfile;

pub fn profile() -> BrowserProfile {
    BrowserProfile {
        id: "chrome-120-windows",
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36",
        platform: "Win32",
    }
}
