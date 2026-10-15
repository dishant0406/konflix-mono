// Gives the Windows executable its icon. gpui embeds its own manifest, so only the icon is added.
fn main() {
    println!("cargo:rerun-if-changed=assets/akidev.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        // The resource compiler runs from the output folder, so the icon is named by its full path.
        let icon = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("assets/akidev.ico");
        let rc = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("akidev.rc");
        std::fs::write(
            &rc,
            format!(
                "1 ICON \"{}\"\n",
                icon.display().to_string().replace('\\', "/")
            ),
        )
        .unwrap();
        embed_resource::compile(rc, embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
