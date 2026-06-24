//! 로컬 영속화: 최고기록(aim_best.txt) + 라운드 CSV 로그(aim_log.csv).
//! 실행 디렉터리에 쓴다. 실패는 조용히 무시.
use std::io::Write;

pub const LOG_CSV: &str = "aim_log.csv";
pub const BEST_FILE: &str = "aim_best.txt";

pub fn load_best() -> f64 {
    std::fs::read_to_string(BEST_FILE)
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

pub fn save_best(best: f64) {
    let _ = std::fs::write(BEST_FILE, format!("{:.2}", best));
}

pub fn append_csv(scenario: &str, dur_s: f64, score: f64, acc: f64) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let new_file = !std::path::Path::new(LOG_CSV).exists();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(LOG_CSV) {
        if new_file {
            let _ = writeln!(f, "epoch,scenario,duration_s,score,accuracy_pct");
        }
        let _ = writeln!(f, "{},{},{:.1},{:.1},{:.1}", ts, scenario, dur_s, score, acc);
    }
}
