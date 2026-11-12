use std::collections::BTreeMap;
pub fn default_node_colors() -> BTreeMap<String, String> {
    [
        ("input", "#5fd5b8"),
        ("output", "#ee8ba0"),
        ("add", "#78a2ff"),
        ("sub", "#78a2ff"),
        ("and", "#b69df9"),
        ("or", "#b69df9"),
        ("xor", "#b69df9"),
        ("not", "#b69df9"),
        ("mux", "#f1bd68"),
        ("register", "#f1bd68"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
pub fn parse_node_color(value: &str) -> Option<[u8; 3]> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return None;
    }
    Some([
        u8::from_str_radix(&value[1..3], 16).ok()?,
        u8::from_str_radix(&value[3..5], 16).ok()?,
        u8::from_str_radix(&value[5..7], 16).ok()?,
    ])
}
