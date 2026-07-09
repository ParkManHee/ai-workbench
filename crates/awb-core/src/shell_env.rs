use std::path::Path;
use std::process::Command;

pub fn login_path() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = Command::new(&shell).args(["-lic", "printf %s \"$PATH\""]).output();
    if let Ok(o) = out {
        let p = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !p.is_empty() { return p; }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:{}/.local/bin", home)
}

pub fn which_in(path: &str, bin: &str) -> Option<String> {
    for dir in path.split(':').filter(|s| !s.is_empty()) {
        let cand = Path::new(dir).join(bin);
        if cand.is_file() { return Some(cand.to_string_lossy().to_string()); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn which_in_finds_executable() {
        let dir = std::env::temp_dir().join("awb_which_test");
        let _ = fs::create_dir_all(&dir);
        let bin = dir.join("mytool");
        fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap(); }
        let found = which_in(dir.to_str().unwrap(), "mytool");
        assert_eq!(found, Some(bin.to_string_lossy().to_string()));
        assert_eq!(which_in(dir.to_str().unwrap(), "nope"), None);
    }
}
