use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct Device { pub id: String, pub token_hash: String, pub label: String, pub paired_at: u64 }

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new(); h.update(s.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone)]
pub struct DeviceStore { pub path: String }

impl DeviceStore {
    pub fn load(path: &str) -> DeviceStore { DeviceStore { path: path.to_string() } }
    fn read(&self) -> Vec<Device> {
        fs::read_to_string(&self.path).ok()
            .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn write(&self, v: &[Device]) {
        if let Ok(s) = serde_json::to_string_pretty(v) { let _ = fs::write(&self.path, s); }
    }
    pub fn list(&self) -> Vec<Device> { self.read() }
    pub fn add(&self, raw_token: &str, label: &str) -> Device {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let id = sha256_hex(&format!("{label}:{now}"))[..12].to_string();
        let dev = Device { id, token_hash: sha256_hex(raw_token), label: label.to_string(), paired_at: now };
        let mut v = self.read(); v.push(dev.clone()); self.write(&v); dev
    }
    pub fn verify(&self, raw_token: &str) -> bool {
        let h = sha256_hex(raw_token);
        self.read().iter().any(|d| d.token_hash == h)
    }
    pub fn remove(&self, id: &str) -> bool {
        let mut v = self.read(); let before = v.len();
        v.retain(|d| d.id != id); self.write(&v); v.len() != before
    }
}

use axum::{extract::State, http::{Request, StatusCode}, middleware::Next, response::Response, body::Body};

pub async fn require_token(
    State(store): State<DeviceStore>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let ok = req.headers().get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| store.verify(t))
        .unwrap_or(false);
    if ok { Ok(next.run(req).await) } else { Err(StatusCode::UNAUTHORIZED) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str) -> String { std::env::temp_dir().join(name).to_string_lossy().to_string() }
    #[test]
    fn add_then_verify_roundtrip() {
        let p = tmp("awb_devices_test1.json"); let _ = std::fs::remove_file(&p);
        let s = DeviceStore::load(&p);
        assert!(!s.verify("tok-abc"));            // 없음
        let d = s.add("tok-abc", "phone1");
        assert!(s.verify("tok-abc"));             // 등록 후 통과
        assert!(!s.verify("tok-xyz"));            // 다른 토큰 거부
        // 원문 저장 안 함(해시만)
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(!raw.contains("tok-abc"));
        assert!(raw.contains(&d.token_hash));
        assert!(s.remove(&d.id));                 // 제거
        assert!(!s.verify("tok-abc"));
    }
}
