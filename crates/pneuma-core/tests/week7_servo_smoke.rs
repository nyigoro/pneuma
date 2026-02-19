use std::fs;
use std::process::Command;

#[test]
#[ignore = "requires SERVO_BIN or SERVO_WEBDRIVER_URL and external network access"]
fn week7_servo_smoke() {
    let has_servo_env =
        std::env::var("SERVO_BIN").is_ok() || std::env::var("SERVO_WEBDRIVER_URL").is_ok();
    if !has_servo_env {
        eprintln!("skipping: set SERVO_BIN or SERVO_WEBDRIVER_URL to run Week 7 smoke test");
        return;
    }

    let script_path = std::env::temp_dir().join(format!("pneuma-week7-servo-{}.js", std::process::id()));
    // NOTE: This uses direct FFI calls for deterministic execution while runtime-level
    // async promise scheduling is still out of scope for Week 7.
    let script = r#"
const nav = JSON.parse(__pneuma_private_ffi.navigate(1, "https://example.com", "{}"));
const title = JSON.parse(__pneuma_private_ffi.evaluate(1, "document.title"));
console.log("title:", title ?? nav.title ?? null);
"#;
    fs::write(&script_path, script).expect("failed to write smoke test script");

    let output = Command::new(env!("CARGO_BIN_EXE_pneuma"))
        .args([
            "run",
            script_path.to_string_lossy().as_ref(),
            "--engine",
            "servo",
        ])
        .env(
            "PNEUMA_LOG",
            "pneuma=info,pneuma_broker=info,pneuma_js=info,pneuma_engines=info,ghost_shim=info",
        )
        .output()
        .expect("failed to run pneuma binary");

    let _ = fs::remove_file(&script_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        output.status.success(),
        "pneuma run failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains("Example Domain"),
        "expected Example Domain in output.\ncombined:\n{combined}"
    );
}
