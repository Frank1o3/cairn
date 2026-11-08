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

fn program_exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}
