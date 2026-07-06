pub fn expand_tilde(p: &str) -> String {
    if p == "~" { return std::env::var("HOME").unwrap_or_else(|_| p.into()); }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") { return format!("{}/{}", home, rest); }
    }
    p.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_slash_path() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~/x"), format!("{}/x", home));
    }

    #[test]
    fn expands_bare_tilde() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~"), home);
    }

    #[test]
    fn leaves_absolute_path_unchanged() {
        assert_eq!(expand_tilde("/abs"), "/abs");
    }

    #[test]
    fn leaves_relative_path_unchanged() {
        assert_eq!(expand_tilde("rel"), "rel");
    }
}
