fn main() {
    let mut attrs = tauri_build::Attributes::new();

    #[cfg(target_os = "windows")]
    {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new()
                .app_manifest(include_str!("brewinget.manifest")),
        );
    }

    tauri_build::try_build(attrs).expect("couldn't run build script");
}
