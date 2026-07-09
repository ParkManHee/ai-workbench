use serde::Serialize;
use std::fs;

#[derive(Serialize, Clone)]
pub struct Badge { pub todo: u32, pub doing: u32, pub done: u32, pub updated: String }

pub fn parse_badge(md: &str) -> Badge {
    let mut section = "";
    let (mut todo, mut doing, mut done) = (0u32, 0u32, 0u32);
    let mut updated = String::new();
    for line in md.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("최종 갱신:") { updated = rest.trim().to_string(); }
        if l.starts_with("## ") {
            section = if l.contains("해야 할 일") {"todo"}
                else if l.contains("진행 중") {"doing"}
                else if l.contains("한 일") {"done"} else {""};
            continue;
        }
        let is_item = l.starts_with("- ");
        if !is_item { continue; }
        let body = l.trim_start_matches("- ").trim_start_matches("[ ]").trim_start_matches("[x]").trim();
        if body.is_empty() || body.starts_with('(') { continue; } // 플레이스홀더 제외
        match section { "todo"=>todo+=1, "doing"=>doing+=1, "done"=>done+=1, _=>{} }
    }
    Badge { todo, doing, done, updated }
}

fn home() -> String { std::env::var("HOME").unwrap_or_default() }

pub fn badge_for(project_name: &str) -> Option<Badge> {
    let base = format!("{}/.claude/worklog", home());
    // 최신 분기 디렉토리(내림차순 첫번째)
    let mut quarters: Vec<String> = fs::read_dir(&base).ok()?
        .flatten().filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("-Q")).collect();
    quarters.sort(); quarters.reverse();
    for q in quarters {
        let p = format!("{}/{}/{}.md", base, q, project_name);
        if let Ok(md) = fs::read_to_string(&p) { return Some(parse_badge(&md)); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_sections() {
        let md = "최종 갱신: 2026-07-06\n## ⬜ 해야 할 일\n- [ ] a\n- [ ] b\n## 🔄 진행 중\n- x\n## ✅ 한 일\n- y\n- z\n- w\n";
        let b = parse_badge(md);
        assert_eq!((b.todo, b.doing, b.done), (2,1,3));
        assert_eq!(b.updated, "2026-07-06");
    }
}
