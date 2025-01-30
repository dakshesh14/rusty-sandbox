use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use std::{fs, thread};

use nix::libc::{prlimit, rlimit, RLIMIT_CPU};
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};

// TODO: skip this function conditional using environment variables
fn configure_cgroups(pid: Pid) {
    let cgroup_path = format!("/sys/fs/cgroup/sandbox_{}", pid);
    fs::create_dir_all(&cgroup_path).expect("Failed to create cgroup directory");

    fs::write(format!("{}/cup.max", cgroup_path), "50000 100000").expect("Failed to set CPU limit");

    fs::write(format!("{}/memory.max", cgroup_path), "134217728")
        .expect("Failed to set memory limit");

    fs::write(format!("{}/cgroup.procs", cgroup_path), pid.to_string())
        .expect("Failed to add process to cgroup");
}

fn set_process_limit(pid: Pid, resource: u32, limit: u64) {
    let rlim = rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };
    let ret = unsafe { prlimit(pid.as_raw(), resource, &rlim, std::ptr::null_mut()) };
    if ret != 0 {
        eprintln!(
            "Failed to set rlimit for PID {}: {}",
            pid,
            std::io::Error::last_os_error()
        );
    }
}

fn create_child_process() -> Option<Pid> {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            unshare(CloneFlags::CLONE_NEWPID).expect("Failed to unshare PID namespace");

            let pid = nix::unistd::getpid();

            println!("Child process PID: {}", pid);

            configure_cgroups(pid);
            set_process_limit(pid, RLIMIT_CPU, 10);

            loop {
                sleep(Duration::from_secs(1));
            }
        }
        Ok(ForkResult::Parent { child }) => {
            println!("Parent created child with PID: {}", child);
            Some(child)
        }
        Err(_) => {
            eprintln!("Failed to fork process");
            None
        }
    }
}

fn is_process_running(pid: Pid) -> bool {
    let path = format!("/proc/{}/status", pid);
    match fs::metadata(path) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn run_echo_in_child(pid: Pid, cmd: &str) {
    let command = format!("nsenter --target {} --pid -- sh -c \"{}\"", pid, cmd);
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn()
        .expect("Failed to run command in child")
        .wait()
        .expect("Failed to wait for command");
}

fn terminate_process(pid: Pid) {
    if let Err(e) = kill(pid, Signal::SIGTERM) {
        eprint!("Failed to kill process: {}", e);
        return;
    }

    for _ in 0..5 {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {
                println!("Process {} exited gracefully", pid);
                return;
            }
            Ok(_) | Err(_) => {}
        }
        sleep(Duration::from_secs(1));
    }
}

fn main() {
    if let Some(pid) = create_child_process() {
        let timeout = 15;
        let start_time = std::time::Instant::now();

        while start_time.elapsed().as_secs() < timeout {
            if is_process_running(pid) {
                run_echo_in_child(pid, "echo 'Hello from child process'");
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }

        terminate_process(pid);
    }
}
