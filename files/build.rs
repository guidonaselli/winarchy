fn main() {
    #[cfg(windows)]
    embed_resource::compile("icon.rc", embed_resource::NONE).manifest_optional().unwrap();
}
