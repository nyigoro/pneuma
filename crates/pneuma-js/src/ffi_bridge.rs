#[cfg(feature = "quickjs")]
use pneuma_broker::handle::BrokerHandle;
#[cfg(feature = "quickjs")]
use rquickjs::{Ctx, Function, Object, Result, Undefined};

#[cfg(feature = "quickjs")]
fn to_js_err(error: anyhow::Error) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("broker", "js", error.to_string())
}

/// Registers all `__pneuma_private_ffi` host functions into the QuickJS context.
/// Must be called BEFORE the ghost_shim.js is evaluated.
#[cfg(feature = "quickjs")]
pub fn register(ctx: Ctx<'_>, broker: BrokerHandle) -> Result<()> {
    let ffi = Object::new(ctx.clone())?;

    ffi.set(
        "print",
        Function::new(ctx.clone(), |msg: String| {
            println!("{msg}");
        })?,
    )?;

    ffi.set(
        "log",
        Function::new(ctx.clone(), |level: String, msg: String| {
            match level.as_str() {
                "warn" => tracing::warn!(target: "ghost_shim", "{msg}"),
                "error" => tracing::error!(target: "ghost_shim", "{msg}"),
                _ => tracing::info!(target: "ghost_shim", "{msg}"),
            }
        })?,
    )?;

    ffi.set("createPage", {
        let broker = broker.clone();
        Function::new(ctx.clone(), move || -> Result<u32> { broker.create_page().map_err(to_js_err) })?
    })?;

    ffi.set("navigate", {
        let broker = broker.clone();
        Function::new(
            ctx.clone(),
            move |page_id: u32, url: String, opts_json: String| -> Result<String> {
                broker.navigate(page_id, url, opts_json).map_err(to_js_err)
            },
        )?
    })?;

    ffi.set("evaluate", {
        let broker = broker.clone();
        Function::new(
            ctx.clone(),
            move |page_id: u32, script: String| -> Result<String> {
                broker.evaluate(page_id, script).map_err(to_js_err)
            },
        )?
    })?;

    ffi.set(
        "screenshot",
        Function::new(ctx.clone(), |page_id: u32| {
            tracing::info!(
                target: "ghost_shim",
                page_id,
                "ffi.screenshot() called - engine not yet wired"
            );
            Undefined
        })?,
    )?;

    ffi.set(
        "closeBrowser",
        Function::new(ctx.clone(), || {
            tracing::info!(target: "ghost_shim", "ffi.closeBrowser() called");
        })?,
    )?;

    ffi.set(
        "exit",
        Function::new(ctx.clone(), |code: Option<i32>| -> () {
            let code = code.unwrap_or(0);
            tracing::info!(target: "ghost_shim", exit_code = code, "ghost.exit() called");
            std::process::exit(code);
        })?,
    )?;

    ctx.globals().set("__pneuma_private_ffi", ffi)?;

    tracing::debug!(target: "pneuma_js", "FFI bridge registered");
    Ok(())
}
