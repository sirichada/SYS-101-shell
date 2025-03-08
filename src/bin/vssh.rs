use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::ffi::CString;
use nix::unistd::fork;
use nix::unistd::ForkResult;
use nix::sys::wait::waitpid;

fn main() -> io::Result<()> {
    loop {
        // Get current working directory
        let current_dir: PathBuf = env::current_dir()?;
        
        // Display the current directory before the prompt
        print!("{}$ ", current_dir.display());
        io::stdout().flush()?; // Ensure the prompt is displayed before waiting for input
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        // Trim the newline character from the end
        let input = input.trim();
        
        // Check if the user wants to exit
        if input.to_lowercase() == "exit" {
            println!("Exiting program...");
            break;
        }

        // skip empty input
        if input.is_empty() {
            continue;
        }

        // if user wants to change directory
        if input.starts_with("cd ") {
            let new_dir = input.trim_start_matches("cd ");
            let path = PathBuf::from(new_dir);
            if path.is_dir() {
                env::set_current_dir(path).unwrap();
            } else {
                eprintln!("Directory does not exist: {}", new_dir);
            }
            continue;
        }

        // for other terminal commands (using nix create)
        match execute_command(input) {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to execute command: {}", e),
        }
    }
    Ok(())
}

fn externalize(command: &str) -> Vec<CString> {
    command.split_whitespace()
        .map(|s| CString::new(s).unwrap())
        .collect()
}

fn execute_command(input: &str) -> io::Result<()> {
    // Check if the command should run in background
    let background = input.trim_end().ends_with('&');
    
    // Process the input string
    let command_str = if background {
        // Remove the & from the end of the command
        input.trim_end().trim_end_matches('&').trim()
    } else {
        input
    };
    
    // Skip if the command is empty after removing &
    if command_str.is_empty() {
        return Ok(());
    }
    
    // Convert the command and arguments to CString
    let command: Vec<CString> = externalize(command_str);

    // Fork the process
    match unsafe { fork() }? {
        ForkResult::Parent { child } => {
            if background {
                // For background processes, print the PID and don't wait
                println!("[{}] Started in background", child);
                Ok(())
            } else {
                // For foreground processes, wait for completion
                waitpid(child, None)?;
                Ok(())
            }
        }
        ForkResult::Child => {
            // Child process: execute the command
            match nix::unistd::execvp(&command[0], &command) {
                Ok(_) => unreachable!(), // execvp replaces the process, so this is never reached on success
                Err(e) => {
                    eprintln!("Failed to execute command: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}