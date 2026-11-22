use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn open_terminal(path: &Path, status: Option<&gtk::Label>) {
    let candidates: &[(&str, &[&str])] = &[
        ("kitty", &["--directory"]),
        ("alacritty", &["--working-directory"]),
        ("foot", &["--working-directory"]),
        ("wezterm", &["start", "--cwd"]),
        ("konsole", &["--workdir"]),
        ("xfce4-terminal", &["--working-directory"]),
        ("gnome-terminal", &["--working-directory"]),
        ("xterm", &["-e"]),
    ];
    for (program, args) in candidates {
        if !program_exists(program) {
            continue;
        }
        let mut command = Command::new(program);
        command.args(*args).current_dir(&path);
        if *program == "wezterm" {
            command.arg(path.as_os_str());
        } else if *program == "xterm" {
            command.arg(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()));
        } else {
            command.arg(path.as_os_str());
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Start a fresh session so the terminal and its shell survive Cairn closing.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        match command.spawn() {
            Ok(_) => {
                if let Some(status) = status {
                    status.set_text(&format!(
                        "Started {program} in {} (detached)",
                        path.display()
                    ));
                }
                return;
            }
            Err(error) => eprintln!("Cairn: failed to launch {program}: {error}"),
        }
    }
    if let Some(status) = status {
        status.set_text("No supported terminal found. Install kitty, alacritty, foot, wezterm, konsole, xfce4-terminal, gnome-terminal, or xterm.");
    }
}

pub fn open_executable_in_terminal(
    executable: &Path,
    working_directory: &Path,
    status: Option<&gtk::Label>,
) {
    const HOLD_OPEN: &str = "\"$@\"; result=$?; printf '\\nProcess exited with status %s. Press Enter to close.' \"$result\"; IFS= read -r _";
    let extension = executable
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let program = match extension.as_str() {
        "py" | "python" => OsString::from("python3"),
        "bash" => OsString::from("/bin/bash"),
        _ => executable.as_os_str().to_os_string(),
    };

    let candidates = [
        "kitty",
        "alacritty",
        "foot",
        "wezterm",
        "konsole",
        "xfce4-terminal",
        "gnome-terminal",
        "xterm",
    ];
    for terminal in candidates {
        if !program_exists(terminal) {
            continue;
        }
        let mut command = Command::new(terminal);
        command.current_dir(working_directory);
        match terminal {
            "kitty" => {
                command.arg("--directory").arg(working_directory).arg("--");
            }
            "alacritty" => {
                command
                    .arg("--working-directory")
                    .arg(working_directory)
                    .arg("-e");
            }
            "foot" => {
                command.arg("--working-directory").arg(working_directory);
            }
            "wezterm" => {
                command
                    .arg("start")
                    .arg("--cwd")
                    .arg(working_directory)
                    .arg("--");
            }
            "konsole" => {
                command.arg("--workdir").arg(working_directory).arg("-e");
            }
            "xfce4-terminal" => {
                command
                    .arg("--working-directory")
                    .arg(working_directory)
                    .arg("--execute");
            }
            "gnome-terminal" => {
                command
                    .arg("--working-directory")
                    .arg(working_directory)
                    .arg("--");
            }
            "xterm" => {
                command.arg("-e");
            }
            _ => unreachable!(),
        }
        command
            .arg("sh")
            .arg("-lc")
            .arg(HOLD_OPEN)
            .arg("cairn-run")
            .arg(&program)
            .arg(executable);
        configure_detached(&mut command);
        match command.spawn() {
            Ok(_) => {
                if let Some(status) = status {
                    status.set_text(&format!("Started {} in {terminal}", executable.display()));
                }
                return;
            }
            Err(error) => eprintln!("Cairn: failed to launch {terminal}: {error}"),
        }
    }
    if let Some(status) = status {
        status.set_text("No supported terminal found. Install kitty, alacritty, foot, wezterm, konsole, xfce4-terminal, gnome-terminal, or xterm.");
    }
}

fn configure_detached(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

fn program_exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}
