use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

use crate::{EngineKind, HeadlessEngine};

const READY_TIMEOUT: Duration = Duration::from_secs(10);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub struct ServoEngine {
    client: reqwest::Client,
    base_url: String,
    session_id: String,
    process: Mutex<Option<Child>>,
}

impl ServoEngine {
    pub async fn launch() -> Result<Self> {
        let client = reqwest::Client::new();
        let (base_url, mut process, port_hint) = match std::env::var("SERVO_WEBDRIVER_URL") {
            Ok(base_url) => {
                let base_url = normalize_base_url(base_url)?;
                tracing::info!(
                    target: "pneuma_engines",
                    base_url = %base_url,
                    "attaching to existing Servo WebDriver endpoint"
                );
                (base_url, None, None)
            }
            Err(_) => {
                let servo_bin = resolve_servo_binary()?;
                let port = allocate_local_port()?;
                let base_url = format!("http://127.0.0.1:{port}");
                let child = Command::new(&servo_bin)
                    .arg(format!("--webdriver={port}"))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| {
                        format!(
                            "failed to launch Servo binary at {}",
                            servo_bin.to_string_lossy()
                        )
                    })?;
                tracing::info!(
                    target: "pneuma_engines",
                    servo_bin = %servo_bin.to_string_lossy(),
                    port,
                    "spawned Servo WebDriver process"
                );
                (base_url, Some(child), Some(port))
            }
        };

        wait_until_ready(&client, &base_url, port_hint, &mut process).await?;
        let session_id = create_session(&client, &base_url).await?;

        tracing::info!(
            target: "pneuma_engines",
            base_url = %base_url,
            session_id = %session_id,
            "Servo WebDriver session created"
        );

        Ok(Self {
            client,
            base_url,
            session_id,
            process: Mutex::new(process),
        })
    }

    fn endpoint(&self, suffix: &str) -> String {
        format!("{}/session/{}/{}", self.base_url, self.session_id, suffix)
    }

    fn session_endpoint(&self) -> String {
        format!("{}/session/{}", self.base_url, self.session_id)
    }
}

#[async_trait]
impl HeadlessEngine for ServoEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Servo
    }

    fn name(&self) -> &'static str {
        "servo"
    }

    async fn navigate(&self, url: &str, opts_json: &str) -> Result<String> {
        tracing::info!(
            target: "pneuma_engines",
            url = %url,
            opts_len = opts_json.len(),
            "Servo navigate"
        );

        let nav_response = self
            .client
            .post(self.endpoint("url"))
            .json(&json!({ "url": url }))
            .send()
            .await
            .context("failed to send Servo WebDriver navigate request")?;
        let nav_status = nav_response.status();
        let nav_body: Value = nav_response
            .json()
            .await
            .context("failed to decode Servo navigate response body")?;
        if !nav_status.is_success() {
            bail!("Servo navigate failed with status {nav_status}: {nav_body}");
        }

        let title_response = self
            .client
            .get(self.endpoint("title"))
            .send()
            .await
            .context("failed to send Servo WebDriver title request")?;
        let title_status = title_response.status();
        let title_body: Value = title_response
            .json()
            .await
            .context("failed to decode Servo title response body")?;
        if !title_status.is_success() {
            bail!("Servo title query failed with status {title_status}: {title_body}");
        }

        let title = title_body
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| title_body.get("value").cloned().unwrap_or(Value::Null).to_string());

        Ok(json!({
            "ok": true,
            "engine": "servo",
            "migrated": false,
            "title": title,
        })
        .to_string())
    }

    async fn evaluate(&self, script: &str) -> Result<String> {
        tracing::info!(
            target: "pneuma_engines",
            script_len = script.len(),
            "Servo evaluate"
        );

        let response = self
            .client
            .post(self.endpoint("execute/sync"))
            .json(&json!({
                "script": "return eval(arguments[0]);",
                "args": [script],
            }))
            .send()
            .await
            .context("failed to send Servo WebDriver evaluate request")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to decode Servo evaluate response body")?;
        if !status.is_success() {
            bail!("Servo evaluate failed with status {status}: {body}");
        }
        let value = body.get("value").cloned().unwrap_or(Value::Null);
        serde_json::to_string(&value).context("failed to encode Servo evaluate result")
    }

    async fn screenshot(&self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn close(&self) -> Result<()> {
        match self.client.delete(self.session_endpoint()).send().await {
            Ok(response)
                if response.status().is_success()
                    || response.status() == reqwest::StatusCode::NOT_FOUND => {}
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_else(|_| "<unreadable>".into());
                tracing::warn!(
                    target: "pneuma_engines",
                    %status,
                    body = %body,
                    "Servo session delete returned non-success"
                );
            }
            Err(error) => {
                tracing::warn!(
                    target: "pneuma_engines",
                    error = %error,
                    "failed to delete Servo WebDriver session"
                );
            }
        }

        let mut process = self.process.lock().await;
        terminate_process(&mut process).await;
        Ok(())
    }
}

fn normalize_base_url(base_url: String) -> Result<String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        bail!("SERVO_WEBDRIVER_URL is set but empty");
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

fn resolve_servo_binary() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("SERVO_BIN") {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            bail!("SERVO_BIN is set but empty");
        }
        return Ok(PathBuf::from(trimmed));
    }
    which::which("servo").map_err(|_| {
        anyhow!(
            "servo binary not found on PATH. Install Servo or set SERVO_BIN to the Servo executable."
        )
    })
}

fn allocate_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .context("failed to bind an ephemeral localhost port for Servo WebDriver")?;
    let port = listener
        .local_addr()
        .context("failed to read ephemeral port for Servo WebDriver")?
        .port();
    drop(listener);
    Ok(port)
}

async fn wait_until_ready(
    client: &reqwest::Client,
    base_url: &str,
    port_hint: Option<u16>,
    process: &mut Option<Child>,
) -> Result<()> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Some(child) = process.as_mut() {
            if let Some(status) = child
                .try_wait()
                .context("failed to check Servo process status during startup")?
            {
                bail!("Servo process exited before WebDriver became ready (status: {status})");
            }
        }

        if Instant::now() > deadline {
            terminate_process(process).await;
            if let Some(port) = port_hint {
                bail!(
                    "Servo WebDriver did not become ready within 10s on port {port}. \
On Linux without a display, try: Xvfb :99 -screen 0 1280x720x24 & DISPLAY=:99 pneuma run ... \
Or set SERVO_WEBDRIVER_URL to point at an already-running instance."
                );
            }
            bail!(
                "Servo WebDriver did not become ready within 10s at {base_url}. \
Set SERVO_WEBDRIVER_URL to a valid endpoint or start Servo manually."
            );
        }

        match client.get(format!("{base_url}/status")).send().await {
            Ok(response) if response.status().is_success() => break,
            _ => sleep(READY_POLL_INTERVAL).await,
        }
    }
    Ok(())
}

async fn create_session(client: &reqwest::Client, base_url: &str) -> Result<String> {
    let session_url = format!("{base_url}/session");
    let w3c_payload = json!({
        "capabilities": {
            "alwaysMatch": {}
        }
    });

    let first_response = client
        .post(&session_url)
        .json(&w3c_payload)
        .send()
        .await
        .context("failed to create WebDriver session with W3C payload")?;

    let response = if !first_response.status().is_success() {
        tracing::debug!(
            target: "pneuma_engines",
            status = %first_response.status(),
            "W3C session creation failed, retrying with legacy payload"
        );
        let legacy_payload = json!({
            "desiredCapabilities": {}
        });
        client
            .post(&session_url)
            .json(&legacy_payload)
            .send()
            .await
            .context("failed to create WebDriver session with legacy payload")?
    } else {
        first_response
    };

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to decode WebDriver session response body")?;

    if !status.is_success() {
        bail!("Servo WebDriver session creation failed with status {status}: {body}");
    }

    body.get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| body.get("value").and_then(|value| value.get("sessionId")).and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Could not extract sessionId from WebDriver response: {body}"))
}

async fn terminate_process(process: &mut Option<Child>) {
    if let Some(mut child) = process.take() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
