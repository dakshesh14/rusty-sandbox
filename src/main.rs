use nix::libc::sleep;
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{fork, ForkResult, Pid};
use std::process::Command;

fn create_child_process() -> Option<Pid> {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            unshare(CloneFlags::CLONE_NEWPID).expect("Failed to unshare PID namespace");
            println!("Child process PID: {}", nix::unistd::getpid());

            loop {
                unsafe { sleep(5) };
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
    if let Err(e) = kill(pid, Signal::SIGKILL) {
        eprintln!("Failed to kill process: {}", e);
    } else {
        println!("Killed process with PID: {}", pid);
    }
}

fn main() {
    if let Some(pid) = create_child_process() {
        unsafe {
            sleep(1);
        }
        run_echo_in_child(pid, "echo 'Hello from child process'");
        terminate_process(pid);
    }
}
