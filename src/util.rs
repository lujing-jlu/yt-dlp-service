pub fn sanitize_filename_component(s: &str) -> String {
    // Keep this conservative: avoid path separators and other odd chars.
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

pub fn video_id_from_url(url: &str) -> Option<String> {
    // Very small heuristic; avoids adding a URL parser dependency.
    if let Some(idx) = url.find("v=") {
        let rest = &url[idx + 2..];
        let id = rest.split('&').next().unwrap_or(rest);
        let id = sanitize_filename_component(id);
        if !id.is_empty() {
            return Some(id);
        }
    }
    if let Some(idx) = url.find("youtu.be/") {
        let rest = &url[idx + "youtu.be/".len()..];
        let id = rest.split('?').next().unwrap_or(rest);
        let id = sanitize_filename_component(id);
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

