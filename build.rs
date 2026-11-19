fn main() {
    println!("cargo:rerun-if-changed=assets/windows/mukade.ico");
    println!("cargo:rerun-if-changed=assets/windows/mukade.rc");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("assets/windows/mukade.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
