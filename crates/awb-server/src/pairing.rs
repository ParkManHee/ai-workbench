use rand::Rng;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct PairingCode { pub code: String, pub expires_at: u64 }

fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

pub fn random_token() -> String {
    let mut b = [0u8; 32]; rand::thread_rng().fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl PairingCode {
    pub fn generate() -> PairingCode {
        const A: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789"; // 혼동문자 제외
        let mut r = rand::thread_rng();
        let code: String = (0..6).map(|_| A[r.gen_range(0..A.len())] as char).collect();
        // 10분: 빌드/설치 후 여유있게 페어링(레이스 방지). tailnet 한정 + 1회성이라 실질 위험 낮음.
        PairingCode { code, expires_at: now() + 600 }
    }
    pub fn is_valid(&self, code: &str, at: u64) -> bool {
        self.code == code && at <= self.expires_at
    }
}

use qrcode::QrCode;
use qrcode::render::unicode;

pub fn render_terminal(payload: &str) -> String {
    match QrCode::new(payload.as_bytes()) {
        Ok(code) => code.render::<unicode::Dense1x2>().quiet_zone(true).build(),
        Err(_) => "(QR 생성 실패)".to_string(),
    }
}

pub fn save_png(payload: &str, path: &str) {
    if let Ok(code) = QrCode::new(payload.as_bytes()) {
        let img = code.render::<image::Luma<u8>>().build();
        let _ = img.save(path);
    }
}

use axum::{extract::{State, Query}, http::StatusCode, Json};
use crate::routes::AppState;

#[derive(serde::Deserialize)]
pub struct PairQuery { pub code: String, pub label: Option<String> }

/// 기기 목록에 보일 이름 — 앱이 보낸 기종 이름(40자까지), 안 보낸 옛 앱은 mobile.
fn device_label(label: Option<&str>) -> String {
    let l: String = label.unwrap_or("").trim().chars().filter(|c| !c.is_control()).take(40).collect();
    if l.is_empty() { "mobile".into() } else { l }
}

#[derive(Serialize)]
pub struct PairResult { pub token: String, pub device_id: String }

pub async fn pair_handler(
    State(st): State<AppState>,
    Query(q): Query<PairQuery>,
) -> Result<Json<PairResult>, StatusCode> {
    let at = now();
    let valid = {
        let mut guard = st.pairing.lock().unwrap();
        let ok = guard.as_ref().map(|pc| pc.is_valid(&q.code, at)).unwrap_or(false);
        if ok { *guard = None; }   // 원자적: 유효할 때만 즉시 소진(동시요청 재사용 차단)
        ok
    };
    if !valid { return Err(StatusCode::FORBIDDEN); }
    let token = random_token();
    let dev = st.devices.add(&token, &device_label(q.label.as_deref()));
    Ok(Json(PairResult { token, device_id: dev.id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_validates_until_expiry() {
        let pc = PairingCode { code: "ABC234".into(), expires_at: 1000 };
        assert!(pc.is_valid("ABC234", 999));
        assert!(pc.is_valid("ABC234", 1000));
        assert!(!pc.is_valid("ABC234", 1001));   // 만료
        assert!(!pc.is_valid("WRONG1", 999));     // 코드 불일치
    }
    #[test]
    fn device_label_uses_sent_model() {
        assert_eq!(device_label(Some(" SM-S918N ")), "SM-S918N");
        assert_eq!(device_label(None), "mobile");
        assert_eq!(device_label(Some("  ")), "mobile");
    }
    #[test]
    fn token_is_64_hex_chars() {
        let t = random_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
