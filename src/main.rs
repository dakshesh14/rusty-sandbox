pub mod sandbox;

use std::thread;
use std::time::Duration;

use crate::sandbox::Sandbox;

fn main() {
    if let Some(sandbox) = Sandbox::new() {
        let timeout = 15;
        let start_time = std::time::Instant::now();

        while start_time.elapsed().as_secs() < timeout {
            if sandbox.is_running() {
                sandbox.run_command("echo 'Hello from child process'");
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }

        sandbox.terminate();
    }
}
