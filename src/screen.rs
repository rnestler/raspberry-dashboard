use std::io::Write;
use std::process::{Command, Stdio};

pub fn cec_standby() {
    run_cec("standby 0");
}

pub fn cec_on() {
    run_cec("on 0");
}

fn run_cec(cmd: &'static str) {
    std::thread::spawn(move || {
        let mut child = match Command::new("cec-client")
            .args(["-s", "-d", "1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("cec-client spawn failed: {e}");
                return;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{cmd}");
        }
        let _ = child.wait();
    });
}
