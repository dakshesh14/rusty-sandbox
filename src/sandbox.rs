use std::env;
use std::fs;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use nix::libc::{prlimit, rlimit, RLIMIT_CPU, RLIMIT_FSIZE};
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};

pub struct Sandbox {
    pid: Pid,
}

impl Sandbox {
    pub fn new() -> Option<Self> {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                unshare(
                    CloneFlags::CLONE_NEWPID
                        | CloneFlags::CLONE_NEWNET
                        | CloneFlags::CLONE_NEWNS
                        | CloneFlags::CLONE_NEWUTS,
                )
                .expect("Failed to unshare PID namespace");

                let pid = nix::unistd::getpid();

                println!("Child process with PID: {}", pid);

                Self::configure_cgroups(pid);

                Self::set_process_limit(pid, RLIMIT_CPU, 10);
                Self::set_process_limit(pid, RLIMIT_FSIZE, 20 * 1024 * 1024);

                loop {
                    sleep(Duration::from_secs(1));
                }
            }
            Ok(ForkResult::Parent { child }) => {
                println!("Parent create child with PID: {}", child);
                Some(Sandbox { pid: child })
            }
            Err(_) => {
                eprintln!("Failed to fork profess");
                None
            }
        }
    }

    fn configure_cgroups(pid: Pid) {
        let enable_cgroups =
            env::var("ENABLE_CGROUPS").unwrap_or_else(|_| "false".into()) == "true";

        if !enable_cgroups {
            eprintln!(
                "\x1b[33mWarning: Cgroups configuration is skipped set ENABLE_CGROUPS=true.\x1b[0m"
            );
            return;
        }

        let cgroup_path = format!("/sys/fs/cgroup/sandbox_{}", pid);
        fs::create_dir_all(&cgroup_path).expect("Failed to create cgroup directory");

        fs::write(format!("{}/cpu.max", cgroup_path), "50000 100000")
            .expect("Failed to set CPU limit");
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
                "Failed to set rlimit for PID: {}: {}",
                pid,
                std::io::Error::last_os_error()
            )
        }
    }

    pub fn is_running(&self) -> bool {
        let path = format!("/proc/{}/status", self.pid);
        match fs::metadata(path) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn run_command(&self, cmd: &str) {
        let command = format!("nsenter --target {} --pid -- sh -c \"{}\"", self.pid, cmd);
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .spawn()
            .expect("Failed to run command in child")
            .wait()
            .expect("Failed to wait for command");
    }

    pub fn terminate(&self) {
        if let Err(e) = kill(self.pid, Signal::SIGTERM) {
            eprintln!("Failed to kill process: {}", e);
            return;
        }

        for _ in 0..5 {
            match waitpid(self.pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {
                    println!("Process {} exited gracefully", self.pid);
                    return;
                }
                Ok(_) | Err(_) => {}
            }
            sleep(Duration::from_secs(1));
        }
    }
}
