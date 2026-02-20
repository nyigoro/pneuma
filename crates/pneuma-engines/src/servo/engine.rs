use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

use crate::{
    EngineKind, HeadlessEngine, LocalStorageEntry, MigrationCookie, MigrationEnvelope,
};

const READY_TIMEOUT: Duration = Duration::from_secs(10);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(200);
const TITLE_READY_TIMEOUT: Duration = Duration::from_secs(2);

static FIRST_EVALUATE_BODY_LOGGED: AtomicBool = AtomicBool::new(false);

pub struct ServoEngine {
    client: reqwest::Client,
    base_url: String,
    session_id: String,
    process: Mutex<Option<Child>>,
}

impl ServoEngine {
    pub async fn launch() -> Result<Self> {
        let client = reqwest::Client::new();
        let (base_url, process, port_hint) = match std::env::var("SERVO_WEBDRIVER_URL") {
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
        Self::initialize(client, base_url, process, port_hint).await
    }

    pub async fn launch_with_endpoint(base_url: String) -> Result<Self> {
        let client = reqwest::Client::new();
        let base_url = normalize_base_url(base_url)?;
        tracing::info!(
            target: "pneuma_engines",
            base_url = %base_url,
            "attaching to explicit secondary Servo WebDriver endpoint"
        );
        Self::initialize(client, base_url, None, None).await
    }

    pub async fn launch_spawned() -> Result<Self> {
        let client = reqwest::Client::new();
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
            "spawned secondary Servo WebDriver process"
        );
        Self::initialize(client, base_url, Some(child), Some(port)).await
    }

    async fn initialize(
        client: reqwest::Client,
        base_url: String,
        mut process: Option<Child>,
        port_hint: Option<u16>,
    ) -> Result<Self> {
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

    async fn collect_probe_metrics(&self) -> Result<Value> {
        let probe_script = r#"(() => {
            const perf = globalThis.performance || {};
            const now = typeof perf.now === 'function' ? Math.round(perf.now()) : 0;
            let firstPaint = null;
            if (typeof perf.getEntriesByType === 'function') {
              const paints = perf.getEntriesByType('paint') || [];
              for (const p of paints) {
                if (p && typeof p.name === 'string' && p.name === 'first-paint') {
                  firstPaint = Math.round(p.startTime || 0);
                  break;
                }
              }
            }
            const nodes = document.querySelectorAll('*');
            let maxDepth = 0;
            for (const node of nodes) {
              let depth = 0;
              let cur = node;
              while (cur && cur.parentElement) {
                depth++;
                cur = cur.parentElement;
              }
              if (depth > maxDepth) maxDepth = depth;
            }
            const bodyTextLength = (document.body && document.body.innerText)
              ? document.body.innerText.trim().length
              : 0;

            return {
              current_url: String(location.href || ''),
              first_paint_ms: firstPaint,
              paint_element_count: nodes.length,
              dom_element_count: nodes.length,
              dom_depth_max: maxDepth,
              body_text_length: bodyTextLength,
              js_execution_time_ms: now,
              js_errors: 0,
              unhandled_promise_rejections: 0,
              console_error_count: 0,
              failed_resource_count: 0,
              cors_violations: 0,
              pending_requests_at_sample: 0,
              css_parse_failures: 0
            };
        })()"#;

        let raw = self.evaluate(probe_script).await?;
        let parsed: Value = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse probe result JSON: {raw}"))?;
        if parsed.is_object() {
            Ok(parsed)
        } else {
            bail!("probe result was not a JSON object: {parsed}");
        }
    }

    async fn fetch_cookies(&self) -> Result<Vec<MigrationCookie>> {
        let response = self
            .client
            .get(self.endpoint("cookie"))
            .send()
            .await
            .context("failed to send WebDriver get cookies request")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to decode WebDriver cookies response")?;
        if !status.is_success() {
            let wd_error = format_wd_error(&body);
            bail!("failed to fetch cookies: status={status}, error={wd_error}, body={body}");
        }

        let value = extract_wd_value(&body)?;
        let mut out = Vec::new();
        let Some(cookies) = value.as_array() else {
            return Ok(out);
        };

        for cookie in cookies {
            let Some(obj) = cookie.as_object() else {
                continue;
            };
            let Some(name) = obj.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(value) = obj.get("value").and_then(Value::as_str) else {
                continue;
            };
            out.push(MigrationCookie {
                name: name.to_string(),
                value: value.to_string(),
                domain: obj.get("domain").and_then(Value::as_str).map(str::to_string),
                path: obj.get("path").and_then(Value::as_str).map(str::to_string),
                secure: obj.get("secure").and_then(Value::as_bool),
                http_only: obj
                    .get("httpOnly")
                    .or_else(|| obj.get("http_only"))
                    .and_then(Value::as_bool),
                expiry: obj.get("expiry").and_then(Value::as_u64),
                same_site: obj
                    .get("sameSite")
                    .or_else(|| obj.get("same_site"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
        Ok(out)
    }

    async fn fetch_local_storage(&self) -> Result<Vec<LocalStorageEntry>> {
        let script =
            "Object.entries(localStorage).map(([key, value]) => ({ key: String(key), value: String(value) }))";
        let raw = self.evaluate(script).await?;
        let parsed: Value = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse localStorage extraction JSON: {raw}"))?;
        let mut out = Vec::new();
        let Some(entries) = parsed.as_array() else {
            return Ok(out);
        };
        for entry in entries {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            let Some(key) = obj.get("key").and_then(Value::as_str) else {
                continue;
            };
            let Some(value) = obj.get("value").and_then(Value::as_str) else {
                continue;
            };
            out.push(LocalStorageEntry {
                key: key.to_string(),
                value: value.to_string(),
            });
        }
        Ok(out)
    }

    async fn import_cookie(&self, cookie: &MigrationCookie) -> Result<()> {
        let mut cookie_obj = serde_json::Map::new();
        cookie_obj.insert("name".into(), Value::String(cookie.name.clone()));
        cookie_obj.insert("value".into(), Value::String(cookie.value.clone()));
        if let Some(v) = &cookie.domain {
            cookie_obj.insert("domain".into(), Value::String(v.clone()));
        }
        if let Some(v) = &cookie.path {
            cookie_obj.insert("path".into(), Value::String(v.clone()));
        }
        if let Some(v) = cookie.secure {
            cookie_obj.insert("secure".into(), Value::Bool(v));
        }
        if let Some(v) = cookie.http_only {
            cookie_obj.insert("httpOnly".into(), Value::Bool(v));
        }
        if let Some(v) = cookie.expiry {
            cookie_obj.insert("expiry".into(), Value::Number(v.into()));
        }
        if let Some(v) = &cookie.same_site {
            cookie_obj.insert("sameSite".into(), Value::String(v.clone()));
        }

        let payload = json!({ "cookie": Value::Object(cookie_obj) });
        let response = self
            .client
            .post(self.endpoint("cookie"))
            .json(&payload)
            .send()
            .await
            .context("failed to send WebDriver add cookie request")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to decode add cookie response")?;
        if !status.is_success() {
            let wd_error = format_wd_error(&body);
            bail!("add cookie failed: status={status}, error={wd_error}, body={body}");
        }
        Ok(())
    }

    async fn import_local_storage_entry(&self, entry: &LocalStorageEntry) -> Result<()> {
        let key_json = serde_json::to_string(&entry.key)
            .context("failed to serialize localStorage key")?;
        let value_json = serde_json::to_string(&entry.value)
            .context("failed to serialize localStorage value")?;
        let script = format!("localStorage.setItem({key_json}, {value_json}); true;");
        let _ = self.evaluate(&script).await?;
        Ok(())
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
            let wd_error = format_wd_error(&nav_body);
            bail!("Servo navigate failed with status {nav_status}: {wd_error}. body={nav_body}");
        }

        let title_endpoint = self.endpoint("title");
        let deadline = Instant::now() + TITLE_READY_TIMEOUT;

        loop {
            let title_response = self
                .client
                .get(&title_endpoint)
                .send()
                .await
                .context("failed to send Servo WebDriver title request")?;
            let title_status = title_response.status();
            let title_body: Value = title_response
                .json()
                .await
                .context("failed to decode Servo title response body")?;

            if title_status.is_success() {
                match extract_wd_value(&title_body) {
                    Ok(title_value) => {
                        let title = title_value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| title_value.to_string());
                        if !title.is_empty() || Instant::now() >= deadline {
                            let mut meta = json!({
                                "ok": true,
                                "engine": "servo",
                                "migrated": false,
                                "title": title,
                            });

                            match self.collect_probe_metrics().await {
                                Ok(probe) => {
                                    if let (Some(meta_obj), Some(probe_obj)) =
                                        (meta.as_object_mut(), probe.as_object())
                                    {
                                        for (key, value) in probe_obj {
                                            meta_obj.insert(key.clone(), value.clone());
                                        }
                                    }
                                }
                                Err(error) => {
                                    tracing::debug!(
                                        target: "pneuma_engines",
                                        error = %error,
                                        "post-navigate probe failed; returning base metadata"
                                    );
                                }
                            }

                            return Ok(meta.to_string());
                        }
                    }
                    Err(error) => {
                        tracing::debug!(
                            target: "pneuma_engines",
                            error = %error,
                            body = ?title_body,
                            "failed to extract title from WebDriver response"
                        );
                    }
                }
            }

            if Instant::now() >= deadline {
                let status = title_status.to_string();
                let wd_error = format_wd_error(&title_body);
                bail!(
                    "Servo title query did not become ready within {}ms after navigate (last_status={status}, error={wd_error}, body={title_body})",
                    TITLE_READY_TIMEOUT.as_millis()
                );
            }
            sleep(READY_POLL_INTERVAL).await;
        }
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

        if FIRST_EVALUATE_BODY_LOGGED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            tracing::debug!(
                target: "pneuma_engines",
                %status,
                body = ?body,
                "first Servo evaluate raw response body"
            );
        }

        if !status.is_success() {
            let wd_error = format_wd_error(&body);
            bail!("Servo evaluate failed with status {status}: {wd_error}. body={body}");
        }

        let value = extract_wd_value(&body)?;
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
                let body: Value = response
                    .json()
                    .await
                    .unwrap_or_else(|_| json!({ "message": "<unreadable response body>" }));
                let wd_error = format_wd_error(&body);
                tracing::warn!(
                    target: "pneuma_engines",
                    %status,
                    error = %wd_error,
                    body = ?body,
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

    async fn extract_state(&self) -> Result<MigrationEnvelope> {
        let captured_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let current_url = match self.evaluate("location.href").await {
            Ok(raw) => serde_json::from_str::<Value>(&raw)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string)),
            Err(error) => {
                tracing::debug!(
                    target: "pneuma_engines",
                    error = %error,
                    "extract_state: failed to evaluate current URL"
                );
                None
            }
        };

        let mut cookie_capture_failed = false;
        let cookies = match self.fetch_cookies().await {
            Ok(cookies) => cookies,
            Err(error) => {
                cookie_capture_failed = true;
                tracing::warn!(
                    target: "pneuma_engines",
                    error = %error,
                    "extract_state: failed to capture cookies"
                );
                Vec::new()
            }
        };

        let mut ls_capture_failed = false;
        let local_storage = match self.fetch_local_storage().await {
            Ok(entries) => entries,
            Err(error) => {
                ls_capture_failed = true;
                tracing::warn!(
                    target: "pneuma_engines",
                    error = %error,
                    "extract_state: failed to capture localStorage"
                );
                Vec::new()
            }
        };

        if cookie_capture_failed && ls_capture_failed {
            bail!("extract_state failed to capture both cookies and localStorage");
        }

        Ok(MigrationEnvelope {
            source_engine: EngineKind::Servo,
            captured_at_ms,
            current_url,
            cookies,
            local_storage,
        })
    }

    async fn import_state(&self, state: MigrationEnvelope) -> Result<()> {
        let cookie_count = state.cookies.len();
        let ls_count = state.local_storage.len();
        let mut cookie_failures: u32 = 0;
        let mut ls_failures: u32 = 0;

        for cookie in &state.cookies {
            if let Err(error) = self.import_cookie(cookie).await {
                cookie_failures = cookie_failures.saturating_add(1);
                tracing::warn!(
                    target: "pneuma_engines",
                    cookie_name = %cookie.name,
                    error = %error,
                    "import_state: failed to import cookie entry"
                );
            }
        }

        for entry in &state.local_storage {
            if let Err(error) = self.import_local_storage_entry(entry).await {
                ls_failures = ls_failures.saturating_add(1);
                tracing::warn!(
                    target: "pneuma_engines",
                    key = %entry.key,
                    error = %error,
                    "import_state: failed to import localStorage entry"
                );
            }
        }

        let total_attempted = cookie_count + ls_count;
        let total_failed = cookie_failures as usize + ls_failures as usize;

        if total_attempted > 0 && total_failed == total_attempted {
            bail!(
                "import_state: all {} attempted imports failed ({} cookies, {} localStorage entries); \
treating as unrecoverable handoff failure",
                total_attempted,
                cookie_failures,
                ls_failures
            );
        }

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
    let attempts = vec![
        ("w3c-bare", json!({ "capabilities": {} })),
        (
            "w3c-full",
            json!({
                "capabilities": {
                    "alwaysMatch": {},
                    "firstMatch": [{}]
                }
            }),
        ),
        ("legacy", json!({ "desiredCapabilities": {} })),
    ];

    let mut last_status = String::new();
    let mut last_error = String::new();
    let mut last_body = Value::Null;
    let mut session_already_started = false;

    for (mode, payload) in attempts {
        let response = client
            .post(&session_url)
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("failed to create WebDriver session ({mode})"))?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .with_context(|| format!("failed to decode session response body ({mode})"))?;

        tracing::debug!(
            target: "pneuma_engines",
            mode,
            %status,
            body = ?body,
            "session creation attempt"
        );

        if status.is_success() {
            return extract_session_id(&body);
        }

        if is_session_already_started(&body) {
            session_already_started = true;
            if let Some(existing_session_id) = body
                .get("sessionId")
                .and_then(Value::as_str)
                .or_else(|| {
                    body.get("value")
                        .and_then(|value| value.get("sessionId"))
                        .and_then(Value::as_str)
                })
            {
                tracing::info!(
                    target: "pneuma_engines",
                    session_id = %existing_session_id,
                    "reusing Servo WebDriver session id from create-session error response"
                );
                return Ok(existing_session_id.to_string());
            }

            if let Some(existing_session_id) = find_existing_session_id(client, base_url).await? {
                tracing::info!(
                    target: "pneuma_engines",
                    session_id = %existing_session_id,
                    "reusing existing Servo WebDriver session"
                );
                return Ok(existing_session_id);
            }
        }

        last_status = status.to_string();
        last_error = format_wd_error(&body);
        last_body = body;
    }

    if session_already_started {
        bail!(
            "Servo WebDriver reports an active session is already running, but this endpoint does not expose a reusable session id. \
Restart the Servo process behind SERVO_WEBDRIVER_URL and retry."
        );
    }

    bail!(
        "Servo WebDriver session creation failed after all attempts. \
Last status: {last_status}, error: {last_error}, body: {last_body}"
    )
}

fn is_session_already_started(body: &Value) -> bool {
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| {
            body.get("value")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    message.to_ascii_lowercase().contains("session is already started")
}

async fn find_existing_session_id(client: &reqwest::Client, base_url: &str) -> Result<Option<String>> {
    let sessions_url = format!("{base_url}/sessions");
    let response = match client.get(&sessions_url).send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(
                target: "pneuma_engines",
                error = %error,
                "failed to query /sessions while attempting session reuse"
            );
            return Ok(None);
        }
    };
    let status = response.status();
    let raw = response.text().await.unwrap_or_default();
    tracing::debug!(
        target: "pneuma_engines",
        %status,
        body = %raw,
        "sessions listing response body"
    );
    if !status.is_success() {
        return Ok(None);
    }

    let body: Value = match serde_json::from_str(&raw) {
        Ok(body) => body,
        Err(error) => {
            tracing::debug!(
                target: "pneuma_engines",
                error = %error,
                "sessions listing was not JSON; cannot reuse existing session"
            );
            return Ok(None);
        }
    };

    let sessions = body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for session in sessions {
        if let Some(session_id) = session
            .get("sessionId")
            .and_then(Value::as_str)
            .or_else(|| session.get("id").and_then(Value::as_str))
            .or_else(|| {
                session
                    .get("value")
                    .and_then(|value| value.get("sessionId"))
                    .and_then(Value::as_str)
            })
        {
            return Ok(Some(session_id.to_string()));
        }
    }
    Ok(None)
}

fn extract_session_id(body: &Value) -> Result<String> {
    body.get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| {
            body.get("value")
                .and_then(|value| value.get("sessionId"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Could not extract sessionId from WebDriver response: {body}"))
}

fn extract_wd_value(body: &Value) -> Result<Value> {
    let value = body
        .get("value")
        .cloned()
        .ok_or_else(|| anyhow!("WebDriver response missing `value`: {body}"))?;

    match value {
        Value::Object(mut object) => {
            if object.len() == 1 && object.contains_key("value") {
                Ok(object.remove("value").unwrap_or(Value::Null))
            } else {
                Ok(Value::Object(object))
            }
        }
        other => Ok(other),
    }
}

fn format_wd_error(body: &Value) -> String {
    let root_error = body.get("error").and_then(Value::as_str);
    let root_message = body.get("message").and_then(Value::as_str);

    let value = body.get("value").and_then(Value::as_object);
    let nested_error = value
        .and_then(|map| map.get("error"))
        .and_then(Value::as_str);
    let nested_message = value
        .and_then(|map| map.get("message"))
        .and_then(Value::as_str);

    let error = nested_error.or(root_error).unwrap_or("unknown WebDriver error");
    let message = nested_message.or(root_message).unwrap_or("no error message");
    format!("{error}: {message}")
}

async fn terminate_process(process: &mut Option<Child>) {
    if let Some(mut child) = process.take() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
