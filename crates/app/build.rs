//! Embeds the icon and file properties into the Windows executable, so
//! Explorer and the taskbar show the app's own icon (the window icon set in
//! `main.rs` only applies once the app runs). Nothing to do elsewhere.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/icon.ico")
        .set("ProductName", "APOST3Dview")
        .set("FileDescription", "APOST3Dview")
        .set("LegalCopyright", "Copyright (C) 2026 Martí Gimferrer");
    resource.compile().expect("the Windows resource file (icon, version) should compile");
}
