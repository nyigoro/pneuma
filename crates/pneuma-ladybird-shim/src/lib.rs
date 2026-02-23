//! Rust-facing surface of the Ladybird C++ shim.
//!
//! Week 16b: `sanity_check()` — proof-of-link sentinel.
//! Week 17:  `LadybirdHandle`, `launch()`, `navigate()` — real browser path.
//!
//! All public items are feature-gated behind `ladybird`.

// ---------------------------------------------------------------------------
// Week 16b: ABI sanity check (unchanged)
// ---------------------------------------------------------------------------

#[cfg(feature = "ladybird")]
extern "C" {
    fn pneuma_ladybird_sanity_check() -> i32;
}

/// Returns `0xCAFE` (51966) when the shim is correctly linked.
#[cfg(feature = "ladybird")]
pub fn sanity_check() -> i32 {
    unsafe { pneuma_ladybird_sanity_check() }
}

// ---------------------------------------------------------------------------
// Week 17: Navigate bridge
// ---------------------------------------------------------------------------

#[cfg(feature = "ladybird")]
mod bridge {
    use anyhow::{anyhow, bail, Context, Result};
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::sync::mpsc;
    use tokio::sync::oneshot;

    // Status codes — must match bridge.cpp constants.
    const PNEUMA_OK: c_int = 0;
    const PNEUMA_INVALID_ARG: c_int = 1;
    const PNEUMA_TIMEOUT: c_int = 2;
    const PNEUMA_RUNTIME_ERR: c_int = 3;

    const NAVIGATE_TIMEOUT_MS: c_int = 30_000;

    // Opaque C++ struct — never dereferenced in Rust.
    #[repr(C)]
    struct PneumaLadybirdBrowser {
        _private: [u8; 0],
    }

    // SAFETY: PneumaLadybirdBrowser is only ever accessed from the single
    // dedicated OS thread. The raw pointer is not shared across threads.
    unsafe impl Send for PneumaLadybirdBrowser {}

    extern "C" {
        fn pneuma_ladybird_browser_create(
            width: c_int,
            height: c_int,
            proxy_server: *const c_char,
        ) -> *mut PneumaLadybirdBrowser;

        fn pneuma_ladybird_navigate(
            browser: *mut PneumaLadybirdBrowser,
            url: *const c_char,
            timeout_ms: c_int,
            out_title: *mut *mut c_char,
            out_error: *mut *mut c_char,
        ) -> c_int;

        fn pneuma_ladybird_evaluate(
            browser: *mut PneumaLadybirdBrowser,
            script: *const c_char,
            timeout_ms: c_int,
            out_result: *mut *mut c_char,
            out_error: *mut *mut c_char,
        ) -> c_int;

        fn pneuma_ladybird_screenshot(
            browser: *mut PneumaLadybirdBrowser,
            full_page: c_int,
            timeout_ms: c_int,
            out_path: *mut *mut c_char,
            out_error: *mut *mut c_char,
        ) -> c_int;

        fn pneuma_ladybird_set_viewport(
            browser: *mut PneumaLadybirdBrowser,
            width: c_int,
            height: c_int,
        ) -> c_int;

        fn pneuma_ladybird_get_viewport(
            browser: *mut PneumaLadybirdBrowser,
            out_width: *mut c_int,
            out_height: *mut c_int,
        ) -> c_int;

        fn pneuma_ladybird_free_string(ptr: *mut c_char);

        fn pneuma_ladybird_browser_destroy(browser: *mut PneumaLadybirdBrowser);
    }

    // -----------------------------------------------------------------------
    // Command channel
    // -----------------------------------------------------------------------

    enum Command {
        Navigate {
            url: String,
            reply: oneshot::Sender<Result<String>>,
        },
        Evaluate {
            script: String,
            reply: oneshot::Sender<Result<String>>,
        },
        Screenshot {
            full_page: bool,
            reply: oneshot::Sender<Result<String>>,
        },
        SetViewport {
            width: u32,
            height: u32,
            reply: oneshot::Sender<Result<()>>,
        },
        GetViewport {
            reply: oneshot::Sender<Result<(u32, u32)>>,
        },
        Shutdown,
    }

    // -----------------------------------------------------------------------
    // LadybirdHandle
    // -----------------------------------------------------------------------

    /// Handle to the dedicated Ladybird OS thread.
    ///
    /// All Ladybird objects live on the thread that owns `Core::EventLoop`.
    /// This handle communicates with that thread via a bounded sync channel.
    pub struct LadybirdHandle {
        tx: mpsc::SyncSender<Command>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for LadybirdHandle {
        fn drop(&mut self) {
            let _ = self.tx.send(Command::Shutdown);
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    /// Launch the Ladybird browser on a dedicated OS thread.
    ///
    /// Blocks until `Application::initialize` and `HeadlessWebView::create`
    /// complete (or fail). Returns an error if initialization fails.
    pub fn launch() -> Result<LadybirdHandle> {
        launch_with_proxy(None)
    }

    /// Launch the Ladybird browser with optional transport proxy routing.
    ///
    /// `proxy_server` must be in `host:port` form if provided.
    pub fn launch_with_proxy(proxy_server: Option<String>) -> Result<LadybirdHandle> {
        let (tx, rx) = mpsc::sync_channel::<Command>(8);
        let (startup_tx, startup_rx) = mpsc::channel::<Result<()>>();

        let thread = std::thread::Builder::new()
            .name("ladybird-eventloop".into())
            .spawn(move || {
                let proxy_server_cstring = match proxy_server {
                    Some(value) => match CString::new(value) {
                        Ok(value) => Some(value),
                        Err(_) => {
                            let _ = startup_tx.send(Err(anyhow!(
                                "proxy server contained interior null byte"
                            )));
                            return;
                        }
                    },
                    None => None,
                };

                // SAFETY: browser is created and used exclusively on this thread.
                let browser = unsafe {
                    pneuma_ladybird_browser_create(
                        1280,
                        720,
                        proxy_server_cstring
                            .as_ref()
                            .map_or(std::ptr::null(), |value| value.as_ptr()),
                    )
                };

                if browser.is_null() {
                    let _ = startup_tx.send(Err(anyhow!(
                        "pneuma_ladybird_browser_create returned null — \
                         check LADYBIRD_BUILD_DIR and host dependencies"
                    )));
                    return;
                }

                let _ = startup_tx.send(Ok(()));

                // Command loop runs on the dedicated thread.
                loop {
                    match rx.recv() {
                        Ok(Command::Navigate { url, reply }) => {
                            let result = do_navigate(browser, &url);
                            let _ = reply.send(result);
                        }
                        Ok(Command::Evaluate { script, reply }) => {
                            let result = do_evaluate(browser, &script);
                            let _ = reply.send(result);
                        }
                        Ok(Command::Screenshot { full_page, reply }) => {
                            let result = do_screenshot(browser, full_page);
                            let _ = reply.send(result);
                        }
                        Ok(Command::SetViewport { width, height, reply }) => {
                            let result = do_set_viewport(browser, width, height);
                            let _ = reply.send(result);
                        }
                        Ok(Command::GetViewport { reply }) => {
                            let result = do_get_viewport(browser);
                            let _ = reply.send(result);
                        }
                        Ok(Command::Shutdown) | Err(_) => break,
                    }
                }

                // Destroy before thread exits.
                unsafe { pneuma_ladybird_browser_destroy(browser) };
            })
            .context("failed to spawn ladybird-eventloop thread")?;

        // Wait for C++ initialization result.
        startup_rx
            .recv()
            .context("ladybird thread exited before reporting startup result")??;

        Ok(LadybirdHandle {
            tx,
            thread: Some(thread),
        })
    }

    /// Navigate to `url` and return the page title.
    pub async fn navigate(handle: &LadybirdHandle, url: String) -> Result<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Navigate {
                url,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Ladybird thread has exited"))?;
        reply_rx
            .await
            .context("Ladybird thread dropped reply sender")?
    }

    /// Evaluate JavaScript in the current page context and return JSON-serialized result.
    pub async fn evaluate(handle: &LadybirdHandle, script: String) -> Result<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Evaluate {
                script,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Ladybird thread has exited"))?;
        reply_rx
            .await
            .context("Ladybird thread dropped reply sender")?
    }

    /// Capture a screenshot and return a file path.
    pub async fn screenshot(handle: &LadybirdHandle, full_page: bool) -> Result<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Screenshot {
                full_page,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Ladybird thread has exited"))?;
        reply_rx
            .await
            .context("Ladybird thread dropped reply sender")?
    }

    /// Set the viewport size (pixels).
    pub async fn set_viewport(handle: &LadybirdHandle, width: u32, height: u32) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(Command::SetViewport {
                width,
                height,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Ladybird thread has exited"))?;
        reply_rx
            .await
            .context("Ladybird thread dropped reply sender")?
    }

    /// Get the viewport size (pixels).
    pub async fn get_viewport(handle: &LadybirdHandle) -> Result<(u32, u32)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(Command::GetViewport { reply: reply_tx })
            .map_err(|_| anyhow!("Ladybird thread has exited"))?;
        reply_rx
            .await
            .context("Ladybird thread dropped reply sender")?
    }

    // -----------------------------------------------------------------------
    // Internal: runs on dedicated Ladybird thread
    // -----------------------------------------------------------------------

    fn do_navigate(browser: *mut PneumaLadybirdBrowser, url: &str) -> Result<String> {
        let url_c = CString::new(url).context("URL contained interior null byte")?;
        let mut out_title: *mut c_char = std::ptr::null_mut();
        let mut out_error: *mut c_char = std::ptr::null_mut();

        let status = unsafe {
            pneuma_ladybird_navigate(
                browser,
                url_c.as_ptr(),
                NAVIGATE_TIMEOUT_MS,
                &mut out_title,
                &mut out_error,
            )
        };

        // Take ownership of a heap-allocated C string and free it.
        let take_string = |ptr: *mut c_char| -> String {
            if ptr.is_null() {
                return String::new();
            }
            let s = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { pneuma_ladybird_free_string(ptr) };
            s
        };

        match status {
            PNEUMA_OK => Ok(take_string(out_title)),
            PNEUMA_TIMEOUT => {
                take_string(out_error); // free even on timeout
                bail!(
                    "Ladybird navigate timed out after {}ms",
                    NAVIGATE_TIMEOUT_MS
                )
            }
            PNEUMA_INVALID_ARG => {
                let msg = take_string(out_error);
                bail!("Ladybird navigate invalid argument: {msg}")
            }
            PNEUMA_RUNTIME_ERR => {
                let msg = take_string(out_error);
                bail!("Ladybird WebContent error: {msg}")
            }
            other => {
                let msg = take_string(out_error);
                bail!("Ladybird navigate unknown status {other}: {msg}")
            }
        }
    }

    fn do_evaluate(browser: *mut PneumaLadybirdBrowser, script: &str) -> Result<String> {
        let script_c = CString::new(script).context("script contained interior null byte")?;
        let mut out_result: *mut c_char = std::ptr::null_mut();
        let mut out_error: *mut c_char = std::ptr::null_mut();

        let status = unsafe {
            pneuma_ladybird_evaluate(
                browser,
                script_c.as_ptr(),
                NAVIGATE_TIMEOUT_MS,
                &mut out_result,
                &mut out_error,
            )
        };

        let take_string = |ptr: *mut c_char| -> String {
            if ptr.is_null() {
                return String::new();
            }
            let s = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { pneuma_ladybird_free_string(ptr) };
            s
        };

        match status {
            PNEUMA_OK => Ok(take_string(out_result)),
            PNEUMA_TIMEOUT => {
                take_string(out_error);
                bail!("Ladybird evaluate timed out after {}ms", NAVIGATE_TIMEOUT_MS)
            }
            PNEUMA_INVALID_ARG => {
                let msg = take_string(out_error);
                bail!("Ladybird evaluate invalid argument: {msg}")
            }
            PNEUMA_RUNTIME_ERR => {
                let msg = take_string(out_error);
                bail!("Ladybird evaluate error: {msg}")
            }
            other => {
                let msg = take_string(out_error);
                bail!("Ladybird evaluate unknown status {other}: {msg}")
            }
        }
    }

    fn do_screenshot(browser: *mut PneumaLadybirdBrowser, full_page: bool) -> Result<String> {
        let mut out_path: *mut c_char = std::ptr::null_mut();
        let mut out_error: *mut c_char = std::ptr::null_mut();

        let status = unsafe {
            pneuma_ladybird_screenshot(
                browser,
                if full_page { 1 } else { 0 },
                NAVIGATE_TIMEOUT_MS,
                &mut out_path,
                &mut out_error,
            )
        };

        let take_string = |ptr: *mut c_char| -> String {
            if ptr.is_null() {
                return String::new();
            }
            let s = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { pneuma_ladybird_free_string(ptr) };
            s
        };

        match status {
            PNEUMA_OK => Ok(take_string(out_path)),
            PNEUMA_TIMEOUT => {
                take_string(out_error);
                bail!("Ladybird screenshot timed out after {}ms", NAVIGATE_TIMEOUT_MS)
            }
            PNEUMA_INVALID_ARG => {
                let msg = take_string(out_error);
                bail!("Ladybird screenshot invalid argument: {msg}")
            }
            PNEUMA_RUNTIME_ERR => {
                let msg = take_string(out_error);
                bail!("Ladybird screenshot error: {msg}")
            }
            other => {
                let msg = take_string(out_error);
                bail!("Ladybird screenshot unknown status {other}: {msg}")
            }
        }
    }

    fn do_set_viewport(browser: *mut PneumaLadybirdBrowser, width: u32, height: u32) -> Result<()> {
        let status = unsafe { pneuma_ladybird_set_viewport(browser, width as c_int, height as c_int) };
        if status == PNEUMA_OK {
            Ok(())
        } else {
            bail!("Ladybird set_viewport failed with status {status}")
        }
    }

    fn do_get_viewport(browser: *mut PneumaLadybirdBrowser) -> Result<(u32, u32)> {
        let mut w: c_int = 0;
        let mut h: c_int = 0;
        let status = unsafe { pneuma_ladybird_get_viewport(browser, &mut w, &mut h) };
        if status == PNEUMA_OK {
            Ok((w as u32, h as u32))
        } else {
            bail!("Ladybird get_viewport failed with status {status}")
        }
    }
}

#[cfg(feature = "ladybird")]
pub use bridge::{
    evaluate, get_viewport, launch, launch_with_proxy, navigate, screenshot, set_viewport,
    LadybirdHandle,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "ladybird"))]
mod tests {
    use super::*;

    #[test]
    fn abi_sanity_check_returns_sentinel() {
        assert_eq!(sanity_check(), 0xCAFE, "Ladybird shim ABI check failed");
    }

    #[test]
    fn launch_then_shutdown_does_not_panic() {
        // Verifies that launch() initializes successfully and Drop
        // shuts down cleanly. Does not load any URL.
        let handle = launch().expect("LadybirdHandle::launch failed");
        drop(handle);
    }

    #[tokio::test]
    #[ignore = "requires working Ladybird build dir and WebContent process"]
    async fn navigate_data_url_returns_title() {
        let handle = launch().expect("LadybirdHandle::launch failed");
        let title = navigate(
            &handle,
            "data:text/html,<title>Week17</title><h1>ok</h1>".into(),
        )
        .await
        .expect("navigate failed");

        assert!(
            title.contains("Week17"),
            "expected title to contain 'Week17', got: {title:?}"
        );
    }
}
