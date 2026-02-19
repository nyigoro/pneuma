fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=shims/ladybird_headless.h");
    println!("cargo:rerun-if-changed=shims/ladybird_headless.cpp");
}
