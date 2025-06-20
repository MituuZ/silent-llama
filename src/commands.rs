/*
 * Copyright © 2025 Mitja Leino
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
 * documentation files (the “Software”), to deal in the Software without restriction, including without limitation
 * the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software,
 * and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
 * WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS
 * OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 *
 */
use crate::history_file::HistoryFile;
use crate::ollama_client::OllamaClient;
use std::collections::HashMap;
use std::process::Command;
use std::{env, fs, io};

pub enum CommandResult {
    Continue,
    Quit,
    SwitchHistory(String),
}

pub struct CommandParams<'a, 'b> {
    args: &'a [&'b str],
    ollama_client: &'a mut OllamaClient,
    history: &'a mut HistoryFile,
    cforge_dir: &'a str,
}

impl<'a, 'b> CommandParams<'a, 'b> {
    pub fn new(
        args: &'a [&'b str],
        ollama_client: &'a mut OllamaClient,
        history: &'a mut HistoryFile,
        cforge_dir: &'a str,
    ) -> Self {
        CommandParams {
            args,
            ollama_client,
            history,
            cforge_dir,
        }
    }
}

type CommandFn = fn(CommandParams) -> io::Result<CommandResult>;

pub struct CommandStruct<'a> {
    command_string: &'a str,
    description: &'a str,
    command_example: Option<&'a str>,
    file_command: bool,
    pub(crate) command_fn: CommandFn,
}

impl<'a> CommandStruct<'a> {
    pub fn new(
        command_string: &'a str,
        description: &'a str,
        command_example: Option<&'a str>,
        file_command: bool,
        command_fn: CommandFn,
    ) -> Self {
        CommandStruct {
            command_string,
            command_example,
            description,
            file_command,
            command_fn,
        }
    }

    pub fn execute(&self, params: CommandParams) -> io::Result<CommandResult> {
        (self.command_fn)(params)
    }

    fn display(&self) -> String {
        match self.command_example {
            Some(example) => format!(
                "{:<12} - {}\n            {}",
                self.command_string, self.description, example
            ),
            None => format!("{:<12} - {}", self.command_string, self.description),
        }
    }
}

pub(crate) fn create_command_registry<'a>() -> HashMap<String, CommandStruct<'a>> {
    let mut commands: HashMap<String, CommandStruct> = HashMap::new();

    commands.insert(
        ":q".to_string(),
        CommandStruct::new(":q", "Exit the program", None, false, quit_command),
    );

    commands.insert(
        ":list".to_string(),
        CommandStruct::new(
            ":list",
            "List files in the cforge directory. \
                    Optionally, you can provide a pattern to filter the results.",
            Some(":list <optional pattern>"),
            true,
            list_command,
        ),
    );

    commands.insert(
        ":switch".to_string(),
        CommandStruct::new(
            ":switch",
            "Switch to a different history file. \
                    Either relative to cforge_dir or absolute path. Creates the file if it doesn't exist.",
            Some(":switch <history file>"),
            true,
            switch_command,
        ),
    );

    commands.insert(
        ":help".to_string(),
        CommandStruct::new(":help", "Show this help message", None, false, help_command),
    );

    commands.insert(
        ":edit".to_string(),
        CommandStruct::new(
            ":edit",
            "Open the history file in your editor",
            None,
            false,
            edit_command,
        ),
    );

    commands.insert(
        ":sysprompt".to_string(),
        CommandStruct::new(
            ":sysprompt",
            "Set the system prompt for current session",
            Some(":sysprompt <prompt>"),
            false,
            sysprompt_command,
        ),
    );

    commands
}

fn quit_command(command_params: CommandParams) -> io::Result<CommandResult> {
    println!(
        "Ending conversation. All interactions saved to '{}'",
        command_params.history.filename
    );
    Ok(CommandResult::Quit)
}

fn list_command(command_params: CommandParams) -> io::Result<CommandResult> {
    let pattern = command_params.args.first().unwrap_or(&"");

    fn list_dir_contents(dir: &str, pattern: &str, cforge_dir: &str) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if (pattern.is_empty() || path.display().to_string().contains(pattern))
                && !path.is_dir()
            {
                match path.display().to_string().strip_prefix(cforge_dir) {
                    None => println!("{}", path.display()),
                    Some(ds) => {
                        let mut cleaned_ds = ds.to_string();
                        if cleaned_ds.starts_with('/') {
                            cleaned_ds = cleaned_ds[1..].to_string();
                        }
                        println!("{}", cleaned_ds)
                    }
                }
            }
            if path.is_dir() {
                list_dir_contents(path.to_str().unwrap(), pattern, cforge_dir)?;
            }
        }
        Ok(())
    }

    list_dir_contents(
        command_params.cforge_dir,
        pattern,
        command_params.cforge_dir,
    )?;
    Ok(CommandResult::Continue)
}

fn help_command(_command_params: CommandParams) -> io::Result<CommandResult> {
    let registry = create_command_registry();
    let mut commands: Vec<&CommandStruct> = registry.values().collect();

    commands.sort_by(|a, b| {
        a.file_command
            .cmp(&b.file_command)
            .then(a.command_string.cmp(b.command_string))
    });

    // Print regular commands first
    println!("General commands:");
    for cmd in &commands {
        if !cmd.file_command {
            println!("{}", cmd.display());
        }
    }

    // Then print file commands
    println!("\nFile commands (supports file completion):");
    for cmd in &commands {
        if cmd.file_command {
            println!("{}", cmd.display());
        }
    }

    Ok(CommandResult::Continue)
}

fn switch_command(command_params: CommandParams) -> io::Result<CommandResult> {
    let new_history_file = command_params.args.first().unwrap_or(&"");

    if new_history_file.is_empty() {
        println!("Error: No history file specified. Usage: :switch <history_file>");
        return Ok(CommandResult::Continue);
    }

    Ok(CommandResult::SwitchHistory(new_history_file.to_string()))
}

fn edit_command(command_params: CommandParams) -> io::Result<CommandResult> {
    let history = command_params.history;
    let editor = env::var("EDITOR")
        .or_else(|_| env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });

    let status = Command::new(editor).arg(history.path.clone()).status();
    if !status.is_ok_and(|s| s.success()) {
        eprintln!("Error opening file in editor");
    } else {
        history.reload_content();
    }

    Ok(CommandResult::Continue)
}

fn sysprompt_command(command_params: CommandParams) -> io::Result<CommandResult> {
    command_params
        .ollama_client
        .update_system_prompt(command_params.args.join(" "));
    Ok(CommandResult::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper function to create the test environment
    fn setup_test_environment() -> (OllamaClient, HistoryFile, TempDir, String) {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_str().unwrap().to_string();

        let ollama_client = OllamaClient::new("test-model".to_string(), "test-prompt".to_string());

        // Create a temporary history file with some content
        let history_path = format!("{}/test-history.txt", dir_path);
        fs::write(&history_path, "Test conversation content").unwrap();

        let history = HistoryFile::new("test-history.txt".to_string(), dir_path.clone()).unwrap();

        (ollama_client, history, temp_dir, dir_path)
    }

    #[test]
    fn test_list_command() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        // Create a few test history files
        fs::write(format!("{}/history1.txt", dir_path), "Content 1")?;
        fs::write(format!("{}/history2.txt", dir_path), "Content 2")?;

        let args: Vec<&str> = vec![];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = list_command(params)?;
        assert!(matches!(result, CommandResult::Continue));

        // We can't easily test the stdout output here without mocking,
        // but the command should run without errors

        Ok(())
    }

    #[test]
    fn test_switch_command() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        // Create a test history file to switch to
        let new_history_file = "new-history.txt";
        fs::write(
            format!("{}/{}", dir_path, new_history_file),
            "New history content",
        )?;

        let args = vec![new_history_file];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = switch_command(params)?;

        if let CommandResult::SwitchHistory(filename) = result {
            assert_eq!(filename, new_history_file);
        } else {
            panic!("Expected SwitchHistory result but got something else");
        }

        Ok(())
    }

    #[test]
    fn test_help_command() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        let args: Vec<&str> = vec![];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = help_command(params)?;
        assert!(matches!(result, CommandResult::Continue));

        Ok(())
    }

    #[test]
    fn test_exit_command() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        let args: Vec<&str> = vec![];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = quit_command(params)?;
        assert!(matches!(result, CommandResult::Quit));

        Ok(())
    }

    #[test]
    fn test_edit_command() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        // We'll mock the editor by setting it to "echo" which should exist on most systems
        // and will just return successfully without doing anything
        unsafe {
            env::set_var("EDITOR", "echo");
        }

        let args: Vec<&str> = vec![];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = edit_command(params)?;
        assert!(matches!(result, CommandResult::Continue));

        Ok(())
    }

    #[test]
    fn test_sysprompt_command() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        let test_prompt = "This is a test system prompt";
        let args: Vec<&str> = test_prompt.split_whitespace().collect();
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = sysprompt_command(params)?;
        assert!(matches!(result, CommandResult::Continue));

        // Verify the prompt was updated
        assert_eq!(ollama_client.system_prompt, test_prompt);

        Ok(())
    }

    #[test]
    fn test_create_command_registry() {
        let registry = create_command_registry();

        // Check that all expected commands are registered
        assert!(registry.contains_key(":q"));
        assert!(registry.contains_key(":list"));
        assert!(registry.contains_key(":switch"));
        assert!(registry.contains_key(":sysprompt"));
        assert!(registry.contains_key(":help"));
        assert!(registry.contains_key(":edit"));

        // Check the total number of commands
        assert_eq!(registry.len(), 6);
    }

    #[test]
    fn test_switch_command_with_no_args() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        let args: Vec<&str> = vec![];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = switch_command(params)?;
        assert!(matches!(result, CommandResult::Continue));

        Ok(())
    }

    #[test]
    fn test_list_command_with_pattern() -> io::Result<()> {
        let (mut ollama_client, mut history, _temp_dir, dir_path) = setup_test_environment();

        // Create some test files
        fs::write(format!("{}/history1.txt", dir_path), "Content 1")?;
        fs::write(format!("{}/history2.txt", dir_path), "Content 2")?;
        fs::write(format!("{}/other.txt", dir_path), "Other content")?;

        // Test with a pattern that should match some files
        let args = vec!["history"];
        let params = CommandParams::new(&args, &mut ollama_client, &mut history, &dir_path);

        let result = list_command(params)?;
        assert!(matches!(result, CommandResult::Continue));

        Ok(())
    }
}
