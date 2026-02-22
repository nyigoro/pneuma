use std::path::{Path, PathBuf};
use std::process::Command;

const REQUIRED_CMAKE: &str = "3.30";
const VCPKG_PINNED_COMMIT: &str = "2fa7118fb2ce0c27ab73e08ab1991f4cb67af880";

fn main() {
    // Keep default workspace builds fast.
    if std::env::var("CARGO_FEATURE_LADYBIRD").is_err() {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("failed to resolve workspace root from CARGO_MANIFEST_DIR")
        .to_path_buf();
    let ladybird_src = workspace_root.join("vendor").join("ladybird");

    let build_dir = std::env::var("LADYBIRD_BUILD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| ladybird_src.join("Build").join("debug-clang20"));
    let vcpkg_root = ladybird_src.join("Build").join("vcpkg");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/sanity.cpp");
    println!("cargo:rerun-if-env-changed=LADYBIRD_BUILD_DIR");

    check_cmake_version(REQUIRED_CMAKE);
    check_tool_present("ninja", "ninja is required (apt package: ninja-build)");
    check_tool_present("nasm", "nasm is required (apt package: nasm)");
    check_tool_present("clang++-20", "clang++-20 is required (apt package: clang-20)");

    if !has_vcpkg_binary(&vcpkg_root) {
        bootstrap_vcpkg(&ladybird_src, &vcpkg_root);
    }

    if !build_dir.join("CMakeCache.txt").exists() {
        std::fs::create_dir_all(&build_dir).expect("failed to create Ladybird build directory");
        cmake_configure(&ladybird_src, &build_dir, &vcpkg_root);
    }

    cmake_build_target(&build_dir, "lib/liblagom-core.so.0");

    let vcpkg_include = build_dir.join("vcpkg_installed").join("x64-linux-dynamic").join("include");
    let lagom_include = build_dir.join("Lagom");

    cc::Build::new()
        .cpp(true)
        .compiler("clang++-20")
        .flag("-std=c++23")
        .include(&ladybird_src)
        .include(&vcpkg_include)
        .include(&lagom_include)
        .file(manifest_dir.join("src").join("sanity.cpp"))
        .compile("pneuma_ladybird_sanity");

    let lib_dir = build_dir.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=lagom-core");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}

fn check_cmake_version(minimum: &str) {
    let output = Command::new("cmake")
        .arg("--version")
        .output()
        .unwrap_or_else(|_| panic!("cmake not found; cmake >= {minimum} is required"));

    if !output.status.success() {
        panic!("failed to execute `cmake --version`");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or_default();
    let found = first_line
        .split_whitespace()
        .find(|part| part.chars().next().map(|ch| ch.is_ascii_digit()).unwrap_or(false))
        .unwrap_or("0.0.0");

    if !version_at_least(found, minimum) {
        panic!(
            "cmake >= {minimum} is required, found `{first_line}`"
        );
    }
}

fn check_tool_present(tool: &str, message: &str) {
    let status = Command::new(tool).arg("--version").status();
    match status {
        Ok(s) if s.success() => {}
        _ => panic!("{message}"),
    }
}

fn has_vcpkg_binary(vcpkg_root: &Path) -> bool {
    vcpkg_root.join("vcpkg").exists() || vcpkg_root.join("vcpkg.exe").exists()
}

fn bootstrap_vcpkg(ladybird_src: &Path, vcpkg_root: &Path) {
    if let Some(parent) = vcpkg_root.parent() {
        std::fs::create_dir_all(parent).expect("failed to create vcpkg parent directory");
    }

    run_cmd(
        Command::new("git")
            .arg("clone")
            .arg("https://github.com/microsoft/vcpkg.git")
            .arg(vcpkg_root)
            .current_dir(ladybird_src),
        "git clone vcpkg",
    );

    run_cmd(
        Command::new("git")
            .arg("checkout")
            .arg(VCPKG_PINNED_COMMIT)
            .current_dir(vcpkg_root),
        "git checkout vcpkg pinned commit",
    );

    run_cmd(
        Command::new("bash")
            .arg(vcpkg_root.join("bootstrap-vcpkg.sh"))
            .arg("-disableMetrics")
            .current_dir(vcpkg_root),
        "bootstrap-vcpkg.sh",
    );
}

fn cmake_configure(ladybird_src: &Path, build_dir: &Path, vcpkg_root: &Path) {
    run_cmd(
        Command::new("cmake")
            .arg("--preset")
            .arg("Debug")
            .arg("-B")
            .arg(build_dir)
            .env("LADYBIRD_SOURCE_DIR", ladybird_src)
            .env("VCPKG_ROOT", vcpkg_root)
            .env("CC", "clang-20")
            .env("CXX", "clang++-20")
            .current_dir(ladybird_src),
        "cmake --preset Debug",
    );
}

fn cmake_build_target(build_dir: &Path, target: &str) {
    run_cmd(
        Command::new("cmake")
            .arg("--build")
            .arg(build_dir)
            .arg("--target")
            .arg(target),
        "cmake --build target",
    );
}

fn run_cmd(cmd: &mut Command, label: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|error| panic!("failed to spawn `{label}`: {error}"));
    if !status.success() {
        panic!("`{label}` failed with status {status}");
    }
}

fn version_at_least(found: &str, minimum: &str) -> bool {
    let parse = |value: &str| -> Vec<u32> {
        value
            .split('.')
            .map(|part| part.chars().take_while(|ch| ch.is_ascii_digit()).collect::<String>())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u32>().ok())
            .collect()
    };

    let mut found_parts = parse(found);
    let mut min_parts = parse(minimum);

    let max_len = found_parts.len().max(min_parts.len());
    found_parts.resize(max_len, 0);
    min_parts.resize(max_len, 0);

    for (f, m) in found_parts.iter().zip(min_parts.iter()) {
        match f.cmp(m) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }

    true
}
