//! ai-workbench 코어 로직 — 데스크톱 앱·데몬이 공유. 전송/UI 비의존 순수 로직.

pub mod lock;
pub mod paths;
pub mod preflight;
pub mod runner;
pub mod runlog;
pub mod scan;
pub mod transcript;
pub mod shell_env;
pub mod worklog;
