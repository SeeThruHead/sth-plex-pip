use std::process::Command;

fn from_pkg_config() -> Vec<String> {
    Command::new("pkg-config")
        .args(["--libs-only-L", "mpv"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|flag| flag.strip_prefix("-L").map(str::to_string))
        .collect()
}

fn main() {
    println!("cargo:rerun-if-env-changed=MPV_LIB_DIR");
    let folders = std::env::var("MPV_LIB_DIR").map(|folder| vec![folder]).unwrap_or_else(|_| from_pkg_config());
    folders.iter().for_each(|folder| println!("cargo:rustc-link-search=native={folder}"));
}
