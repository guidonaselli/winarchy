use super::Theme;

#[test]
fn generated_theme_preserves_urgent_independently_of_accent() {
    let json = r##"{"background":"#1a1b26","opacity":1.0,"foreground":"#ffffff","accent":"#7aa2f7","urgent":"#f7768e","muted":"#111111","font_family":"A","font_size":12,"position":"top"}"##;
    let theme: Theme = serde_json::from_str(json).unwrap();
    assert_eq!(theme.urgent.as_deref(), Some("#f7768e"));
}

#[test]
fn legacy_theme_without_urgent_keeps_accent_available_for_fallback() {
    let json = r##"{"background":"#1a1b26","opacity":1.0,"foreground":"#ffffff","accent":"#7aa2f7","muted":"#111111","font_family":"A","font_size":12,"position":"top"}"##;
    let theme: Theme = serde_json::from_str(json).unwrap();
    assert_eq!((theme.urgent.as_deref(), theme.accent.as_str()), (None, "#7aa2f7"));
}
