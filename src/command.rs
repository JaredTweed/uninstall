use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tempfile::tempfile;
use wait_timeout::ChildExt;

static DEBUG: AtomicBool = AtomicBool::new(false);

pub fn set_debug(enabled: bool) {
    DEBUG.store(enabled, Ordering::Release);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    Ok,
    NonZero,
    Timeout,
    Missing,
}

#[derive(Debug)]
pub struct CommandResult {
    pub status: CommandStatus,
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl CommandResult {
    pub fn ok(&self) -> bool {
        self.status == CommandStatus::Ok
    }

    pub fn completed(&self) -> bool {
        !matches!(self.status, CommandStatus::Timeout | CommandStatus::Missing)
    }

    pub fn combined(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

fn read_file(mut file: File) -> String {
    let _ = file.seek(SeekFrom::Start(0));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn run<I, S>(program: &str, args: I, timeout: Duration) -> CommandResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let started = Instant::now();
    let stdout_file = match tempfile() {
        Ok(file) => file,
        Err(error) => {
            return CommandResult {
                status: CommandStatus::Missing,
                code: 127,
                stdout: String::new(),
                stderr: error.to_string(),
                duration: started.elapsed(),
            };
        }
    };
    let stderr_file = match tempfile() {
        Ok(file) => file,
        Err(error) => {
            return CommandResult {
                status: CommandStatus::Missing,
                code: 127,
                stdout: String::new(),
                stderr: error.to_string(),
                duration: started.elapsed(),
            };
        }
    };
    let stdout_child = match stdout_file.try_clone() {
        Ok(file) => file,
        Err(error) => {
            return CommandResult {
                status: CommandStatus::Missing,
                code: 127,
                stdout: String::new(),
                stderr: error.to_string(),
                duration: started.elapsed(),
            };
        }
    };
    let stderr_child = match stderr_file.try_clone() {
        Ok(file) => file,
        Err(error) => {
            return CommandResult {
                status: CommandStatus::Missing,
                code: 127,
                stdout: String::new(),
                stderr: error.to_string(),
                duration: started.elapsed(),
            };
        }
    };
    let mut child = match Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_child))
        .stderr(Stdio::from(stderr_child))
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return CommandResult {
                status: CommandStatus::Missing,
                code: 127,
                stdout: String::new(),
                stderr: error.to_string(),
                duration: started.elapsed(),
            };
        }
    };
    let (status, exit_status): (CommandStatus, Option<ExitStatus>) =
        match child.wait_timeout(timeout) {
            Ok(Some(exit)) => (
                if exit.success() {
                    CommandStatus::Ok
                } else {
                    CommandStatus::NonZero
                },
                Some(exit),
            ),
            Ok(None) => {
                let _ = child.kill();
                let exit = child.wait().ok();
                (CommandStatus::Timeout, exit)
            }
            Err(_) => {
                let _ = child.kill();
                let exit = child.wait().ok();
                (CommandStatus::Missing, exit)
            }
        };
    let result = CommandResult {
        status,
        code: if status == CommandStatus::Timeout {
            124
        } else {
            exit_status.and_then(|value| value.code()).unwrap_or(127)
        },
        stdout: read_file(stdout_file),
        stderr: read_file(stderr_file),
        duration: started.elapsed(),
    };
    if DEBUG.load(Ordering::Acquire) {
        eprintln!(
            "debug: {} finished in {:.3}s (status {:?}, exit {})",
            crate::util::sanitize(program),
            result.duration.as_secs_f64(),
            result.status,
            result.code
        );
    }
    result
}

pub fn output(program: &str, args: &[&str]) -> String {
    let result = run(program, args, Duration::from_secs(30));
    if result.ok() {
        result.stdout
    } else {
        String::new()
    }
}

pub fn exists(program: &str) -> bool {
    which(program).is_some()
}

pub fn which(program: &str) -> Option<std::path::PathBuf> {
    if program.contains('/') {
        let path = std::path::PathBuf::from(program);
        return path
            .metadata()
            .ok()
            .filter(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .map(|_| path);
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(program))
            .find(|candidate| {
                candidate.metadata().is_ok_and(|metadata| {
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                })
            })
    })
}
