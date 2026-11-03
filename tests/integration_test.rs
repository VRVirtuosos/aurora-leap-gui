use std::path::PathBuf;
use std::time::Duration;

#[test]
fn test_live_viewer_handshake_and_ping() {
    let candidate_paths = [
        PathBuf::from("../Aurora-Viewer/target/debug/aurora-viewer.exe"),
        PathBuf::from("Aurora-Viewer/target/debug/aurora-viewer.exe"),
        PathBuf::from("../../Aurora-Viewer/target/debug/aurora-viewer.exe"),
    ];

    let mut exe_path = None;
    for p in candidate_paths {
        if p.exists() {
            exe_path = Some(p.canonicalize().unwrap());
            break;
        }
    }

    let exe = match exe_path {
        Some(e) => e,
        None => {
            eprintln!("Skipping live viewer integration test: aurora-viewer.exe not found");
            return;
        }
    };

    println!("Testing against live binary: {}", exe.display());

    // We can run the leap_test_runner or test via client
    // Let's spawn process and verify handshake
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--headless");
    cmd.arg("--leap-stdio");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn aurora-viewer");
    let mut stdout = child.stdout.take().unwrap();

    // Read initial handshake packet
    let mut decoder = aurora_leap_gui_codec_decoder();
    let mut buf = [0u8; 1024];
    let t0 = std::time::Instant::now();
    let mut got_handshake = false;

    while t0.elapsed() < Duration::from_secs(10) {
        use std::io::Read;
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                decoder.feed(&buf[..n]);
                if let Ok(Some(msg)) = decoder.decode_next() {
                    println!("Received initial frame on pump '{}'", msg.pump);
                    if let serde_llsd::LLSDValue::Map(ref map) = msg.data {
                        if map.contains_key("command") {
                            println!("Verified command pump: {:?}", map.get("command"));
                            got_handshake = true;
                            break;
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    assert!(got_handshake, "Failed to receive LEAP handshake from aurora-viewer.exe");
}

fn aurora_leap_gui_codec_decoder() -> aurora_leap_gui_decoder_shim::Decoder {
    aurora_leap_gui_decoder_shim::Decoder::new()
}

mod aurora_leap_gui_decoder_shim {
    use serde_llsd::LLSDValue;

    pub struct Decoder {
        buffer: Vec<u8>,
    }

    impl Decoder {
        pub fn new() -> Self {
            Self { buffer: Vec::new() }
        }

        pub fn feed(&mut self, chunk: &[u8]) {
            self.buffer.extend_from_slice(chunk);
        }

        pub fn decode_next(&mut self) -> Result<Option<Msg>, String> {
            let colon_idx = match self.buffer.iter().position(|&b| b == b':') {
                Some(idx) => idx,
                None => return Ok(None),
            };

            let prefix = std::str::from_utf8(&self.buffer[..colon_idx]).map_err(|e| e.to_string())?;
            let len: usize = prefix.trim().parse().map_err(|e| format!("{e}"))?;

            let start = colon_idx + 1;
            let end = start + len;
            if self.buffer.len() < end {
                return Ok(None);
            }

            let payload_str = std::str::from_utf8(&self.buffer[start..end]).map_err(|e| e.to_string())?;
            let val = serde_llsd::notation_from_str(payload_str.trim_start()).map_err(|e| e.to_string())?;

            self.buffer.drain(..end);

            if let LLSDValue::Map(mut map) = val {
                let pump = match map.remove("pump") {
                    Some(LLSDValue::String(s)) => s,
                    _ => "unknown".to_string(),
                };
                let data = map.remove("data").unwrap_or(LLSDValue::Undefined);
                Ok(Some(Msg { pump, data }))
            } else {
                Err("Not a map".to_string())
            }
        }
    }

    pub struct Msg {
        pub pump: String,
        pub data: LLSDValue,
    }
}
