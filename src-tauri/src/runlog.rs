use serde::Serialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom};

#[derive(Serialize, Clone)]
pub struct LogChunk {
    pub text: String,
    pub offset: u64,
    pub done: bool,
    pub exit_code: Option<i32>,
}

pub fn read_log(log: &str, offset: u64) -> LogChunk {
    let mut text = String::new();
    let mut new_off = offset;
    if let Ok(mut f) = fs::File::open(log) {
        if f.seek(SeekFrom::Start(offset)).is_ok() {
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() {
                new_off = offset + buf.len() as u64;
                text = String::from_utf8_lossy(&buf).into_owned();
            }
        }
    }
    let done_path = format!("{log}.done");
    let (done, exit_code) = match fs::read_to_string(&done_path) {
        Ok(s) => (true, s.trim().parse::<i32>().ok()),
        Err(_) => (false, None),
    };
    LogChunk {
        text,
        offset: new_off,
        done,
        exit_code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_and_done() {
        let log = std::env::temp_dir().join("awb_tail.log");
        // Cleanup from previous runs
        let _ = std::fs::remove_file(&log);
        let done_file = format!("{}.done", log.to_str().unwrap());
        let _ = std::fs::remove_file(&done_file);

        std::fs::write(&log, "line1\n").unwrap();
        let c1 = read_log(log.to_str().unwrap(), 0);
        assert_eq!(c1.text, "line1\n");
        assert!(!c1.done);
        std::fs::write(&log, "line1\nline2\n").unwrap();
        let c2 = read_log(log.to_str().unwrap(), c1.offset);
        assert_eq!(c2.text, "line2\n");
        std::fs::write(&done_file, "0\n").unwrap();
        let c3 = read_log(log.to_str().unwrap(), c2.offset);
        assert!(c3.done);
        assert_eq!(c3.exit_code, Some(0));

        // Cleanup
        let _ = std::fs::remove_file(&log);
        let _ = std::fs::remove_file(&done_file);
    }
}
