use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio, Child};
use std::fs::File;
use nix::unistd::fork;
use nix::unistd::ForkResult;
use nix::sys::wait::waitpid;

// Struct to represent a parsed command line
struct CommandLine {
    background: bool,
    input_file: Option<String>,
    output_file: Option<String>,
    commands: Vec<String>,
}

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

        // Skip empty input
        if input.is_empty() {
            continue;
        }

        // If user wants to change directory
        if input.starts_with("cd ") {
            let new_dir = input.trim_start_matches("cd ");
            let path = PathBuf::from(new_dir);
            if path.is_dir() {
                match env::set_current_dir(path) {
                    Ok(_) => (),
                    Err(e) => eprintln!("Error changing directory: {}", e),
                }
            } else {
                eprintln!("Directory does not exist: {}", new_dir);
            }
            continue;
        }

        // For terminal commands
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                // Check if the command should run in background
                let background = input.trim_end().ends_with('&');
                
                if background {
                    // For background processes, print the PID and don't wait
                    println!("[{}] Started in background", child);
                } else {
                    // For foreground processes, wait for completion
                    match waitpid(child, None) {
                        Ok(_) => (),
                        Err(e) => eprintln!("Error waiting for child process: {}", e),
                    }
                }
            },
            Ok(ForkResult::Child) => {
                // Child process: execute the command
                let result = execute_command(input);
                match result {
                    Ok(_) => (),
                    Err(e) => eprintln!("Failed to execute command: {}", e),
                }
                std::process::exit(0);
            },
            Err(e) => eprintln!("Fork failed: {}", e),
        }
    }
    Ok(())
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

    // Parse the command line
    let cmd_line = parse_command(command_str)?;
    
    // Execute based on whether it's a pipeline or single command
    if cmd_line.commands.len() == 1 {
        execute_single_command(&cmd_line)
    } else {
        execute_pipeline(&cmd_line)
    }
}

fn parse_command(input: &str) -> io::Result<CommandLine> {
    // Split by pipe for pipeline commands
    let pipeline_stages: Vec<&str> = input.split('|').map(|s| s.trim()).collect();
    let mut commands = Vec::new();
    
    // Process each pipeline stage
    for stage in pipeline_stages {
        commands.push(stage.to_string());
    }
    
    // Check for input redirection in the first command
    let mut input_file = None;
    if !commands.is_empty() {
        let first_cmd = commands[0].clone();
        if first_cmd.contains(" < ") {
            let parts: Vec<&str> = first_cmd.splitn(2, " < ").collect();
            if parts.len() == 2 {
                commands[0] = parts[0].to_string();
                input_file = Some(parts[1].trim().to_string());
            }
        }
    }
    
    // Check for output redirection in the last command
    let mut output_file = None;
    if !commands.is_empty() {
        let last_idx = commands.len() - 1;
        let last_cmd = commands[last_idx].clone();
        if last_cmd.contains(" > ") {
            let parts: Vec<&str> = last_cmd.splitn(2, " > ").collect();
            if parts.len() == 2 {
                commands[last_idx] = parts[0].to_string();
                output_file = Some(parts[1].trim().to_string());
            }
        }
    }
    
    // Create and return the command line object
    Ok(CommandLine {
        background: false,  // Already handled in the main function
        input_file,
        output_file,
        commands,
    })
}

fn execute_single_command(cmd_line: &CommandLine) -> io::Result<()> {
    // Parse the command and arguments
    let cmd_parts: Vec<&str> = cmd_line.commands[0].split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Ok(());
    }
    
    let program = cmd_parts[0];
    let args = &cmd_parts[1..];
    
    let mut command = Command::new(program);
    command.args(args);
    
    // Handle input redirection
    if let Some(input_file) = &cmd_line.input_file {
        match File::open(input_file) {
            Ok(file) => {
                command.stdin(Stdio::from(file));
            },
            Err(_) => {
                return Err(io::Error::new(io::ErrorKind::NotFound, 
                                         format!("Input file not found: {}", input_file)));
            }
        }
    }
    
    // Handle output redirection
    if let Some(output_file) = &cmd_line.output_file {
        match File::create(output_file) {
            Ok(file) => {
                command.stdout(Stdio::from(file));
            },
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::Other, 
                                         format!("Failed to create output file: {}", e)));
            }
        }
    }
    
    // Execute the command and wait for it to complete
    match command.status() {
        Ok(_) => Ok(()),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, 
                                    format!("Failed to execute command: {}", e)))
    }
}

fn execute_pipeline(cmd_line: &CommandLine) -> io::Result<()> {
    let mut commands = Vec::new();
    let mut children: Vec<Child> = Vec::new();
    
    // Create Command objects for each pipeline stage
    for cmd_str in &cmd_line.commands {
        let cmd_parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if cmd_parts.is_empty() {
            continue;
        }
        
        let mut cmd = Command::new(cmd_parts[0]);
        cmd.args(&cmd_parts[1..]);
        commands.push(cmd);
    }
    
    // Handle input redirection for the first command
    if let Some(input_file) = &cmd_line.input_file {
        match File::open(input_file) {
            Ok(file) => {
                if !commands.is_empty() {
                    commands[0].stdin(Stdio::from(file));
                }
            },
            Err(_) => {
                return Err(io::Error::new(io::ErrorKind::NotFound, 
                                         format!("Input file not found: {}", input_file)));
            }
        }
    }
    
    // Handle output redirection for the last command
    if let Some(output_file) = &cmd_line.output_file {
        if !commands.is_empty() {
            match File::create(output_file) {
                Ok(file) => {
                    let last_idx = commands.len() - 1;
                    commands[last_idx].stdout(Stdio::from(file));
                },
                Err(e) => {
                    return Err(io::Error::new(io::ErrorKind::Other, 
                                             format!("Failed to create output file: {}", e)));
                }
            }
        }
    }
    
    // Execute the pipeline
    if commands.is_empty() {
        return Ok(());
    }
    
    // Setup the pipeline
    for i in 0..commands.len() - 1 {
        commands[i].stdout(Stdio::piped());
        
        if i > 0 {
            // Get stdout from previous command
            let previous_stdout = children[i-1].stdout.take()
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get stdout from previous command"))?;
            
            commands[i].stdin(Stdio::from(previous_stdout));
        }
        
        // Spawn the command
        let child = commands[i].spawn()?;
        children.push(child);
    }
    
    // Handle the last command
    let last_idx = commands.len() - 1;
    
    if last_idx > 0 {
        // Get stdout from previous command
        let previous_stdout = children[last_idx-1].stdout.take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get stdout from previous command"))?;
        
        commands[last_idx].stdin(Stdio::from(previous_stdout));
    }
    
    // Execute the last command and wait for completion
    match commands[last_idx].status() {
        Ok(_) => Ok(()),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("Failed to execute command: {}", e)))
    }
}