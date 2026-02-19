use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct SessionCookieJar {
    cookies: HashMap<String, String>,
}

impl SessionCookieJar {
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.cookies.insert(name.into(), value.into());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }
}
