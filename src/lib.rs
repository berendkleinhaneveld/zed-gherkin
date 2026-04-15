use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct GherkinExtension;

impl zed::Extension for GherkinExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let path = worktree.which("gherkin-lsp").ok_or_else(|| {
            "gherkin-lsp not found on PATH. Install with: cargo install --path crates/gherkin-lsp"
                .to_string()
        })?;
        Ok(Command {
            command: path,
            args: vec![],
            env: Default::default(),
        })
    }
}

zed::register_extension!(GherkinExtension);
