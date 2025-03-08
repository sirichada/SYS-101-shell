use std::io::{self, Write};
use std::env;
use std::path::PathBuf;

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
        
        println!("You entered: {}", input);
    }
    
    Ok(())
}