use super::BrowserProfile;

pub fn profile() -> BrowserProfile {
    BrowserProfile {
        id: "firefox-121-linux",
        user_agent: "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
        platform: "Linux x86_64",
    }
}
