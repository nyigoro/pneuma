use anyhow::Result;
use reqwest::Client;

use crate::stealth::identity::BrowserIdentity;

#[derive(Debug, Clone)]
pub struct NetworkInterceptor {
    client: Client,
    identity: BrowserIdentity,
}

impl NetworkInterceptor {
    pub fn new(identity: BrowserIdentity) -> Result<Self> {
        let client = Client::builder().cookie_store(true).build()?;
        Ok(Self { client, identity })
    }

    pub fn identity(&self) -> &BrowserIdentity {
        &self.identity
    }

    pub async fn get_text(&self, url: &str) -> Result<String> {
        let response = self.client.get(url).send().await?;
        Ok(response.text().await?)
    }
}
