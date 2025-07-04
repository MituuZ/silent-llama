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

use crate::command_complete::CommandHelper;
use rustyline::history::DefaultHistory;
use rustyline::{Cmd, Editor, EventHandler, KeyEvent, Modifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{fs, io};

#[derive(Debug, Clone, Copy, Deserialize, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    #[default]
    Emacs,
    Vi,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionType {
    #[default]
    Circular,
    List,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct RustylineConfig {
    #[serde(default)]
    pub edit_mode: EditMode,

    #[serde(default)]
    pub completion_type: CompletionType,
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_model")]
    pub(crate) model: String,

    #[serde(default = "default_cforge_dir")]
    pub(crate) cforge_dir: String,

    #[serde(default = "default_system_prompt")]
    pub(crate) system_prompt: String,

    #[serde(default)]
    pub(crate) rustyline: RustylineConfig,

    #[serde(default = "default_token_estimation")]
    pub(crate) token_estimation: bool,

    pub(crate) last_history_file: Option<String>,
}

fn default_model() -> String {
    "gemma3:12b".to_string()
}

fn default_token_estimation() -> bool {
    true
}

fn default_cforge_dir() -> String {
    get_home_dir()
        .map(|home_dir| home_dir.join("cforge").display().to_string())
        .unwrap_or_else(|_| {
            eprintln!("Could not determine home directory, using current directory instead.");
            "./cforge".to_string()
        })
}

fn default_system_prompt() -> String {
    r#"
    You are an AI assistant receiving input from a command-line
    application called convo-forge (cforge). The user may include additional context from another file,
    this is included as a separate user prompt.
    Your responses are displayed in the terminal and saved to the history file.
    Keep your answers helpful, concise, and relevant to both the user's direct query and any file context provided.
    \n\n"#.to_string()
}

impl Config {
    pub fn default() -> Self {
        Self {
            model: default_model(),
            cforge_dir: default_cforge_dir(),
            system_prompt: default_system_prompt(),
            rustyline: RustylineConfig::default(),
            token_estimation: default_token_estimation(),
            last_history_file: None,
        }
    }

    pub fn create_rustyline_config(&self) -> rustyline::Config {
        let mut config_builder = rustyline::Config::builder();

        // Apply edit mode setting
        config_builder = match self.rustyline.edit_mode {
            EditMode::Emacs => config_builder.edit_mode(rustyline::EditMode::Emacs),
            EditMode::Vi => config_builder.edit_mode(rustyline::EditMode::Vi),
        };

        config_builder = match self.rustyline.completion_type {
            CompletionType::Circular => {
                config_builder.completion_type(rustyline::CompletionType::Circular)
            }
            CompletionType::List => config_builder.completion_type(rustyline::CompletionType::List),
        };

        config_builder.build()
    }

    pub fn create_editor(&self) -> rustyline::Result<Editor<CommandHelper, DefaultHistory>> {
        let config = self.create_rustyline_config();
        let commands = vec!["q", "help", "list", "switch", "edit", "sysprompt"];
        let file_commands = vec![":list", ":switch"];
        let helper = CommandHelper::new(commands, file_commands, &self.cforge_dir);
        let mut editor = Editor::with_config(config)?;
        editor.set_helper(Some(helper));

        editor.bind_sequence(
            KeyEvent(rustyline::KeyCode::Enter, Modifiers::ALT),
            EventHandler::Simple(Cmd::Newline),
        );

        Ok(editor)
    }

    pub fn load() -> io::Result<Self> {
        let config_path = match get_config_path() {
            Ok(path) => path,
            Err(e) => {
                eprintln!("Couldn't load config_path: {}", e);
                println!("Using default config values.");
                return Ok(Config::default());
            }
        };

        let config_str = match fs::read_to_string(&config_path) {
            Ok(config_string) => config_string,
            Err(s) => {
                eprintln!("Could not read config file: {}", s);
                println!("Using default config values.");
                return Ok(Config::default());
            }
        };

        // This will automatically use the default values for any missing fields
        toml::from_str(&config_str).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub(crate) fn update_last_history_file(&mut self, history_file: String) -> io::Result<()> {
        self.last_history_file = Some(history_file);
        self.save()
    }

    pub(crate) fn save(&self) -> io::Result<()> {
        let config_path =
            get_config_path().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let config_str = toml::to_string(self).map_err(io::Error::other)?;
        fs::write(config_path, config_str)
    }
}

fn get_config_path() -> Result<PathBuf, &'static str> {
    match get_home_dir() {
        Ok(home_dir) => Ok(home_dir.join(".cforge.toml")),
        Err(_) => Err("Error loading config path: Could not determine home directory"),
    }
}

fn get_home_dir() -> Result<PathBuf, &'static str> {
    if let Ok(home) = std::env::var("HOME") {
        Ok(PathBuf::from(home))
    } else if cfg!(windows) && std::env::var("USERPROFILE").is_ok() {
        Ok(PathBuf::from(std::env::var("USERPROFILE").unwrap()))
    } else {
        return Err("Could not determine home directory");
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, EditMode};

    #[test]
    fn test_default_config_values() {
        let config = Config::default();

        // Check default values directly
        assert_eq!(config.model, "gemma3:12b");
        assert!(config.system_prompt.contains("You are an AI assistant"));

        // For cforge_dir, just check that it ends with "/cforge" or "\cforge"
        // rather than testing the specific home directory path
        assert!(config.cforge_dir.ends_with("/cforge") || config.cforge_dir.ends_with("\\cforge"));

        // Check rustyline default values
        matches!(config.rustyline.edit_mode, EditMode::Emacs);
        matches!(
            config.rustyline.completion_type,
            crate::config::CompletionType::Circular
        );
    }
}
