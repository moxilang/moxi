use std::collections::HashMap;

/// Returns a HashMap of built-in color names to hex values.
pub fn default_colors() -> HashMap<String, String> {
    HashMap::from([
        ("red".to_string(), "#ff0000".to_string()),
        ("orange".to_string(), "#ffa500".to_string()),
        ("yellow".to_string(), "#ffff00".to_string()),
        ("green".to_string(), "#00ff00".to_string()),
        ("blue".to_string(), "#0000ff".to_string()),
        ("purple".to_string(), "#800080".to_string()),
        ("white".to_string(), "#ffffff".to_string()),
        ("black".to_string(), "#000000".to_string()),
        ("brown".to_string(), "#8b4513".to_string()),
        ("moxi-pink".to_string(), "#fcb7b7".to_string()),
    ])
}
