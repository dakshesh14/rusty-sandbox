use nix::libc;
use nix::sched::{unshare, CloneFlags};
use nix::sys::resource::{getrlimit, setrlimit, Resource};
use nix::unistd::{fork, ForkResult};
use std::process::{Command, Stdio};

fn get_system_memory_limit() -> u64 {
    match getrlimit(Resource::RLIMIT_AS) {
        Ok((soft, hard)) => soft.unwrap_or(hard.unwrap_or(0)) as u64,
        Err(_) => 0,
    }
}

fn set_memory_limit(limit_kb: u64) -> Result<(), nix::Error> {
    let soft_limit = Some(limit_kb as libc::rlim_t);
    let hard_limit = Some(limit_kb as libc::rlim_t);
    setrlimit(Resource::RLIMIT_AS, soft_limit, hard_limit)?;
    Ok(())
}

fn run_python_in_isolation(code: &str, original_limit: u64) {
    unshare(CloneFlags::CLONE_NEWNET | CloneFlags::CLONE_NEWPID)
        .expect("Failed to isolate process");

    let output = Command::new("python3")
        .arg("-c")
        .arg(code)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute Python");

    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    set_memory_limit(original_limit).expect("Failed to reset memory limit");
}

fn run_echo_in_child(original_limit: u64) {
    set_memory_limit(20 * 1024 * 1024).expect("Failed to set memory limit");

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            Command::new("echo")
                .arg("Hello from child process!")
                .spawn()
                .expect("Failed to run echo")
                .wait()
                .expect("Failed to wait for echo");
        }
        Ok(ForkResult::Parent { .. }) => {
            println!("Parent process, waiting for child...");
        }
        Err(e) => eprintln!("Fork failed: {}", e),
    }

    set_memory_limit(original_limit).expect("Failed to reset memory limit");
}

fn main() {
    let original_limit = get_system_memory_limit();
    let code = r#"
print("Hello from Python!")
"#;
    run_echo_in_child(original_limit);
    run_python_in_isolation(code, original_limit);
    println!("Hello, world!!!");
}
