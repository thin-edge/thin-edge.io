fn main() {
    // export GIT_SEMVER=$(git describe --always --tags --abbrev=8 --dirty)
    // https://github.com/rust-lang/cargo/issues/6583#issuecomment-1259871885
    if let Ok(val) = std::env::var("GIT_SEMVER") {
        println!("Using version defined by 'GIT_SEMVER={}'", val);
        println!("cargo:rustc-env=CARGO_PKG_VERSION={}", val);
    }
    println!("cargo:rerun-if-env-changed=GIT_SEMVER");
}
