// crates/awb-server/src/power.rs (Task 6) — /awake 전원 어서션: caffeinate 자식 프로세스 관리
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PowerGuard { child: Arc<Mutex<Option<Child>>> }

impl PowerGuard {
    pub fn new() -> PowerGuard { PowerGuard { child: Arc::new(Mutex::new(None)) } }
    pub fn is_active(&self) -> bool { self.child.lock().unwrap().is_some() }
    pub fn set(&self, on: bool) {
        let mut g = self.child.lock().unwrap();
        if on {
            if g.is_none() {
                // macOS: caffeinate -s (AC 전원 시 시스템 슬립 방지). 비-macOS는 TODO.
                if let Ok(c) = Command::new("caffeinate").arg("-s").spawn() { *g = Some(c); }
            }
        } else if let Some(mut c) = g.take() {
            let _ = c.kill(); let _ = c.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggle_spawns_and_kills() {
        let pg = PowerGuard::new();
        assert!(!pg.is_active());
        pg.set(true);
        assert!(pg.is_active());   // caffeinate 자식 존재(macOS 가정; caffeinate 없으면 이 테스트는 macOS 전용)
        pg.set(false);
        assert!(!pg.is_active());
    }
}
