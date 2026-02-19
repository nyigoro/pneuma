use rquickjs::{Ctx, Function, Object, Result, Undefined};

/// Registers all `__pneuma_private_ffi` host functions into the QuickJS context.
/// Must be called BEFORE the ghost_shim.js is evaluated.
pub fn register(ctx: Ctx<'_>) -> Result<()> {
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

    ffi.set(
        "createPage",
        Function::new(ctx.clone(), || {
            tracing::info!(target: "ghost_shim", "ffi.createPage() called");
            1_u32
        })?,
    )?;

    ffi.set(
        "navigate",
        Function::new(ctx.clone(), |page_id: u32, url: String, opts_json: String| {
            tracing::info!(
                target: "ghost_shim",
                page_id,
                url = %url,
                opts_len = opts_json.len(),
                "ffi.navigate() called - engine not yet wired"
            );
            r#"{"ok":true,"engine":"stub","migrated":false}"#.to_string()
        })?,
    )?;

    ffi.set(
        "evaluate",
        Function::new(ctx.clone(), |page_id: u32, script: String| {
            tracing::info!(
                target: "ghost_shim",
                page_id,
                script_len = script.len(),
                "ffi.evaluate() called - engine not yet wired"
            );
            "null".to_string()
        })?,
    )?;

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
