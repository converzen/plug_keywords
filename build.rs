use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap_or_default();

    // Fallback to "unknown" if git fails, otherwise take 8 chars
    let short_hash = if git_hash.len() >= 8 {
        &git_hash[..8]
    } else {
        "unknown"
    };
    // This line tells Cargo to set an environment variable for the build
    println!("cargo:rustc-env=GIT_HASH={}", &short_hash);

    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
}
