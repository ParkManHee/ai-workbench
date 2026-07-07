// crates/awb-server/src/routes.rs (Task 3에서 최소 선언, Task 6에서 확장)
use std::sync::{Arc, Mutex};
use crate::auth::DeviceStore;
use crate::pairing::PairingCode;

#[derive(Clone)]
pub struct AppState {
    pub devices: DeviceStore,
    pub pairing: Arc<Mutex<Option<PairingCode>>>,
}
