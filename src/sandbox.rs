use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use libc::prctl;
use libc::PR_SET_SECCOMP;
use libc::SECCOMP_MODE_FILTER;
use nix::libc::{prlimit, rlimit, RLIMIT_CPU, RLIMIT_FSIZE};
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::chroot;
use nix::unistd::{fork, ForkResult, Pid};

use seccomp::Compare;
use seccomp::Context;
use seccomp::{Action, Rule};

use crate::config::constants::Settings;

pub struct Sandbox {
    pid: Pid,
}

impl Sandbox {
    /// Creates a new sandboxed process using `fork()`.
    /// The child process enters isolated namespaces and applies cgroups and resource limits.
    /// Returns `Some(Sandbox)` if successful, otherwise `None`.
    pub fn new() -> Option<Self> {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                unshare(
                    CloneFlags::CLONE_NEWPID
                        | CloneFlags::CLONE_NEWNET
                        | CloneFlags::CLONE_NEWNS
                        | CloneFlags::CLONE_NEWUTS
                        | CloneFlags::CLONE_NEWIPC
                        | CloneFlags::CLONE_NEWUSER,
                )
                .expect("Failed to unshare PID namespace");

                let pid = nix::unistd::getpid();

                println!("Child process with PID: {}", pid);

                if (Settings::from_env().use_complete_isolation) {
                    Self::configure_cgroups(pid);
                    Self::isolate_filesystem(pid);
                    Self::drop_root_privileges();
                    Self::apply_seccomp(pid);
                    Self::limit_process_count(pid);
                    Self::disable_network(pid);
                } else {
                    eprintln!("\x1b[33mWarning: Not using complete isolation setup!\x1b[0m");
                }

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
                eprintln!("Failed to fork process");
                None
            }
        }
    }

    /// Drops root privileges by setting up UID and GID mappings and disabling setgroups.
    /// This prevents the sandboxed process from gaining root privileges.
    fn drop_root_privileges() {
        fs::write("/proc/self/setgroups", "deny").expect("Failed to disable setgroups.");
        fs::write("/proc/self/uid_map", "1000 1000 1").expect("Failed to set UID map");
        fs::write("/proc/self/gid_map", "1000 1000 1").expect("Failed to set UID map");
    }

    /// Isolates the filesystem by using `chroot` to set the root directory for the process.
    /// The process will only be able to access files within this new root directory.
    fn isolate_filesystem(pid: Pid) {
        let root_dir = format!("sandbox/{}/root", pid);
        chroot(root_dir.as_str()).expect("Failed to chroot");
        std::env::set_current_dir("/").expect("Failed to change directory.");
    }

    /// Limits the process count for the sandboxed process by configuring the cgroup for PIDs.
    /// This prevents the process from creating an excessive number of child processes.
    fn limit_process_count(pid: Pid) {
        let cgroup_path = format!("/sys/fs/cgroup/sandbox_{}/pids.max", pid);
        if !Path::new(&cgroup_path).exists() {
            fs::create_dir_all(&cgroup_path).expect("Failed to create cgroup directory");
        }

        fs::write(cgroup_path, "20").expect("Failed to set process limit");
    }

    /// Disables network access for the sandboxed process by configuring the cgroup to block networking.
    /// This ensures the process cannot access the internet or other network resources.
    fn disable_network(pid: Pid) {
        let net_cls = format!("/sys/fs/cgroup/sandbox_{}/net_cls.classid", pid);
        fs::write(net_cls, "0").expect("Failed to disable network access");
    }

    /// Applies a seccomp filter to restrict the system calls that the sandboxed process can make.
    /// This is done to prevent the process from performing harmful or dangerous operations.
    fn apply_seccomp(pid: Pid) {
        let mut ctx =
            Context::default(Action::Kill).expect("Error occurred while setting context.");

        let read_rule = Rule::new(
            0,
            Compare::arg(0)
                .using(seccomp::Op::Ge)
                .with(0)
                .build()
                .unwrap(),
            Action::Allow,
        );
        ctx.add_rule(read_rule).expect("Failed to set read rule.");

        let write_rule = Rule::new(
            1,
            Compare::arg(0)
                .using(seccomp::Op::Ge)
                .with(0)
                .build()
                .unwrap(),
            Action::Allow,
        );
        ctx.add_rule(write_rule).expect("Failed to set write rule.");

        let exit_rule = Rule::new(
            60,
            Compare::arg(0)
                .using(seccomp::Op::Ge)
                .with(0)
                .build()
                .unwrap(),
            Action::Allow,
        );
        ctx.add_rule(exit_rule).expect("Failed to set exit rule.");

        ctx.load().expect("Failed to load context");

        unsafe {
            let res = prctl(
                PR_SET_SECCOMP,
                SECCOMP_MODE_FILTER,
                pid.as_raw() as u32,
                0,
                0,
            );
            if res != 0 {
                eprintln!("Failed to apply seccomp filter to PID {}", pid);
            }
        }
    }

    /// Configures cgroups for the given process ID (`pid`).
    /// CPU and memory limits are applied if `ENABLE_CGROUPS=true` is set in the environment.
    fn configure_cgroups(pid: Pid) {
        let cgroup_path = format!("/sys/fs/cgroup/sandbox_{}", pid);
        fs::create_dir_all(&cgroup_path).expect("Failed to create cgroup directory");

        fs::write(format!("{}/cpu.max", cgroup_path), "50000 100000")
            .expect("Failed to set CPU limit");
        fs::write(format!("{}/memory.max", cgroup_path), "134217728")
            .expect("Failed to set memory limit");
        fs::write(format!("{}/cgroup.procs", cgroup_path), pid.to_string())
            .expect("Failed to add process to cgroup");
    }

    /// Sets resource limits (e.g., CPU time, file size) for a process.
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

    /// Checks if the sandboxed process is still running.
    /// Returns `true` if the process is active, otherwise `false`.
    pub fn is_running(&self) -> bool {
        let path = format!("/proc/{}/status", self.pid);
        match fs::metadata(path) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Runs a command inside the sandboxed process using `nsenter`.
    pub fn run_command(&self, cmd: &str) -> Result<String, String> {
        let command = format!("nsenter --target {} --pid -- sh -c \"{}\"", self.pid, cmd);
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(|e| format!("{}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    /// Terminates the sandboxed process gracefully using `SIGTERM`.
    /// Waits up to 5 seconds for the process to exit.
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
