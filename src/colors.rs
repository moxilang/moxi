use std::collections::HashMap;

pub fn default_colors() -> HashMap<String, String> {
    HashMap::from([
        ("red".to_string(),        "#ff0000".to_string()),
        ("orange".to_string(),     "#ffa500".to_string()),
        ("yellow".to_string(),     "#ffff00".to_string()),
        ("green".to_string(),      "#00ff00".to_string()),
        ("blue".to_string(),       "#0000ff".to_string()),
        ("purple".to_string(),     "#800080".to_string()),
        ("white".to_string(),      "#ffffff".to_string()),
        ("black".to_string(),      "#000000".to_string()),
        ("gray".to_string(),       "#8f8f8f".to_string()),
        ("grey".to_string(),       "#4d4d4d".to_string()),
        ("brown".to_string(),      "#8b4513".to_string()),
        ("ivory".to_string(),      "#fffff0".to_string()),
        ("maroon".to_string(),     "#800000".to_string()),
        ("peach".to_string(),      "#ffcba4".to_string()),
        ("mochi-pink".to_string(), "#fcb7b7".to_string()),
    ])
}

/// Resolve a color name or pass through a hex string.
/// "ivory" → "#fffff0",  "#ff0000" → "#ff0000"
pub fn resolve_color(name: &str) -> String {
    if name.starts_with('#') {
        return name.to_string();
    }
    default_colors()
        .get(name)
        .cloned()
        .unwrap_or_else(|| "#ffffff".to_string())
}
