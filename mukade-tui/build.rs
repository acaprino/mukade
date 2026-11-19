fn main() {
    println!("cargo:rerun-if-changed=assets/windows/mukade-tui.ico");
    println!("cargo:rerun-if-changed=assets/windows/mukade-tui.rc");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("assets/windows/mukade-tui.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
