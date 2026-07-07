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
        PairingCode { code, expires_at: now() + 60 }
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
pub struct PairQuery { pub code: String }

#[derive(Serialize)]
pub struct PairResult { pub token: String, pub device_id: String }

pub async fn pair_handler(
    State(st): State<AppState>,
    Query(q): Query<PairQuery>,
) -> Result<Json<PairResult>, StatusCode> {
    let at = now();
    let valid = { st.pairing.lock().unwrap().as_ref().map(|pc| pc.is_valid(&q.code, at)).unwrap_or(false) };
    if !valid { return Err(StatusCode::FORBIDDEN); }
    // 1회성: 사용 후 코드 무효화
    *st.pairing.lock().unwrap() = None;
    let token = random_token();
    let dev = st.devices.add(&token, "mobile");
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
    fn token_is_64_hex_chars() {
        let t = random_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
