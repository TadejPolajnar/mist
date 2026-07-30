use std::process::Command;

/// The benchmark doubles as a regression guard for the performance thesis:
/// path-precise writes must stay within a small constant of hand-written
/// setData, and orders of magnitude under naive resends.
#[test]
fn bridge_traffic_stays_path_precise() {
    let out = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/benchmark/bench.js"))
        .output()
        .expect("node not found — benchmark requires Node.js");
    assert!(out.status.success(), "bench failed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("RESULT ")).expect("no RESULT line");
    let json = line.trim_start_matches("RESULT ");

    let get = |key: &str| -> f64 {
        let idx = json.find(key).expect(key);
        let rest = &json[idx + key.len() + 2..];
        rest.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap()
    };
    let mist = get("mistToggleAvg");
    let optimal = get("optimalToggleAvg");
    let naive = get("naiveToggleAvg");
    let calls = get("mistCalls");

    // one batched setData per toggle
    assert_eq!(calls as u64, 100, "stdout:\n{}", stdout);
    // within a small constant of the hand-written floor (item write via keyed diff)
    assert!(mist <= optimal * 8.0, "mist {} vs optimal {}", mist, optimal);
    // orders of magnitude under naive resends
    assert!(naive / mist > 100.0, "naive {} vs mist {}", naive, mist);
}
