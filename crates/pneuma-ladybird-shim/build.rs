use std::path::{Path, PathBuf};
use std::process::Command;

const REQUIRED_CMAKE: &str = "3.30";
const VCPKG_PINNED_COMMIT: &str = "2fa7118fb2ce0c27ab73e08ab1991f4cb67af880";
const HELPER_SERVICES: &[&str] = &["RequestServer", "ImageDecoder", "WebContent"];
const LAGOM_SHARED_LIBS: &[&str] = &[
    "lagom-ak",
    "lagom-compress",
    "lagom-core",
    "lagom-coreminimal",
    "lagom-crypto",
    "lagom-database",
    "lagom-devtools",
    "lagom-filesystem",
    "lagom-gc",
    "lagom-gfx",
    "lagom-http",
    "lagom-idl",
    "lagom-imagedecoderclient",
    "lagom-ipc",
    "lagom-js",
    "lagom-media",
    "lagom-regex",
    "lagom-requests",
    "lagom-syntax",
    "lagom-textcodec",
    "lagom-threading",
    "lagom-tls",
    "lagom-unicode",
    "lagom-url",
    "lagom-wasm",
    "lagom-web",
    "lagom-webview",
    "lagom-xml",
];

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
    println!("cargo:rerun-if-changed=src/bridge.cpp");
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
    eprintln!("[pneuma-ladybird-shim] building liblagom-webview...");
    run_cmd(
        Command::new("cmake")
            .arg("--build")
            .arg(&build_dir)
            .args(["--target", "lib/liblagom-webview.so.0"])
            .current_dir(&ladybird_src),
        "cmake build liblagom-webview",
    );
    for target in HELPER_SERVICES {
        cmake_build_target(&build_dir, target);
    }
    link_helper_services_into_target_libexec(&build_dir, HELPER_SERVICES);

    let vcpkg_include = build_dir.join("vcpkg_installed").join("x64-linux-dynamic").join("include");
    let lagom_include = build_dir.join("Lagom");

    cc::Build::new()
        .cpp(true)
        .compiler("clang++-20")
        .flag("-std=c++23")
        .flag("-Wno-unqualified-std-cast-call")
        .include(&ladybird_src)
        .include(ladybird_src.join("Libraries"))
        .include(&vcpkg_include)
        .include(&lagom_include)
        .include(lagom_include.join("Libraries"))
        .include(lagom_include.join("Services"))
        .file(manifest_dir.join("src").join("sanity.cpp"))
        .file(manifest_dir.join("src").join("bridge.cpp"))
        .compile("pneuma_ladybird_sanity");

    let lib_dir = build_dir.join("lib");
    let staged_lib_dir = stage_lagom_runtime_libs(&lib_dir, LAGOM_SHARED_LIBS);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-search=native={}", staged_lib_dir.display());
    // Lagom shared libraries - full set required to satisfy all symbol references.
    for lib in LAGOM_SHARED_LIBS {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
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

fn stage_lagom_runtime_libs(source_lib_dir: &Path, lagom_libs: &[&str]) -> PathBuf {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("missing OUT_DIR"));
    let staged_lib_dir = out_dir.join("lagom-runtime-lib");
    std::fs::create_dir_all(&staged_lib_dir).expect("failed to create staged lagom runtime lib directory");

    let entries = std::fs::read_dir(source_lib_dir)
        .unwrap_or_else(|error| panic!("failed to read lagom lib directory {}: {error}", source_lib_dir.display()));
    let names: Vec<String> = entries
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("failed to read lagom lib entry in {}: {error}", source_lib_dir.display()))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    for lib in lagom_libs {
        let needle = format!("lib{lib}.so");
        let mut matched = false;
        for name in names.iter().filter(|name| name.starts_with(&needle)) {
            matched = true;
            let source = source_lib_dir.join(name);
            let destination = staged_lib_dir.join(name);
            if std::fs::symlink_metadata(&destination).is_ok() {
                std::fs::remove_file(&destination).unwrap_or_else(|error| {
                    panic!("failed to replace staged lagom runtime lib {}: {error}", destination.display())
                });
            }
            create_link_or_copy(&source, &destination);
        }

        if !matched {
            panic!(
                "required lagom shared library `{needle}` not found in {}",
                source_lib_dir.display()
            );
        }
    }

    staged_lib_dir
}

fn link_helper_services_into_target_libexec(build_dir: &Path, services: &[&str]) {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("missing OUT_DIR"));
    let target_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("failed to resolve Cargo target dir from OUT_DIR");
    let libexec_dir = target_dir.join("libexec");
    std::fs::create_dir_all(&libexec_dir).expect("failed to create cargo target libexec directory");

    for service in services {
        let source = build_dir.join("libexec").join(service);
        if !source.exists() {
            panic!("helper service binary not found: {}", source.display());
        }

        let link = libexec_dir.join(service);
        if std::fs::symlink_metadata(&link).is_ok() {
            std::fs::remove_file(&link).unwrap_or_else(|error| {
                panic!("failed to remove existing helper link {}: {error}", link.display())
            });
        }

        create_helper_service_link_or_copy(&source, &link);
    }
}

#[cfg(unix)]
fn create_helper_service_link_or_copy(source: &Path, link: &Path) {
    create_link_or_copy(source, link);
}

#[cfg(not(unix))]
fn create_helper_service_link_or_copy(source: &Path, link: &Path) {
    create_link_or_copy(source, link);
}

#[cfg(unix)]
fn create_link_or_copy(source: &Path, destination: &Path) {
    std::os::unix::fs::symlink(source, destination).unwrap_or_else(|error| {
        panic!(
            "failed to create symlink {} -> {}: {error}",
            destination.display(),
            source.display()
        )
    });
}

#[cfg(not(unix))]
fn create_link_or_copy(source: &Path, destination: &Path) {
    std::fs::copy(source, destination).unwrap_or_else(|error| {
        panic!(
            "failed to copy file {} -> {}: {error}",
            source.display(),
            destination.display()
        )
    });
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
