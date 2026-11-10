//! Running `$EDITOR` for notes and whole-item JSON.
//!
//! The caller suspends the terminal before calling [`edit_text`] and restores
//! it afterwards. The buffer is written to a 0600 file in the private runtime
//! directory and shredded after use.

use crate::security;
use std::path::Path;
use std::process::Command;

/// Why the editor could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    /// No editor command could be resolved.
    NoEditor,
    /// The temporary file could not be created or read.
    Io(String),
    /// The editor exited non-zero.
    Failed(String),
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEditor => {
                f.write_str("No editor found: set [editor] command, $VISUAL or $EDITOR")
            }
            Self::Io(message) => write!(f, "Couldn't use the editor: {message}"),
            Self::Failed(message) => write!(f, "The editor failed: {message}"),
        }
    }
}

impl std::error::Error for EditorError {}

/// Resolves the editor: config → `$VISUAL` → `$EDITOR` → `vi`.
#[must_use]
pub fn resolve(configured: &str) -> String {
    if !configured.trim().is_empty() {
        return configured.trim().to_owned();
    }
    for variable in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(variable)
            && !value.trim().is_empty()
        {
            return value.trim().to_owned();
        }
    }
    "vi".to_owned()
}

/// Runs the editor on `text` and returns the edited text.
pub fn edit_text(text: &str, configured: &str) -> Result<String, EditorError> {
    let editor = resolve(configured);
    let (path, mut file) =
        security::secure_temp_file("edit").map_err(|e| EditorError::Io(e.to_string()))?;
    {
        use std::io::Write;
        file.write_all(text.as_bytes()).map_err(|e| EditorError::Io(e.to_string()))?;
        file.flush().map_err(|e| EditorError::Io(e.to_string()))?;
    }
    drop(file);
    let result = run_editor(&editor, &path);
    let edited = std::fs::read_to_string(&path).map_err(|e| EditorError::Io(e.to_string()));
    let _ = security::shred_and_remove(&path);
    result?;
    edited
}

/// Extra arguments that stop well-known editors from writing the secret
/// buffer anywhere else on disk (swap, undo, backup and history files).
#[must_use]
pub fn hardening_args(editor: &str) -> &'static str {
    let program = editor.split_whitespace().next().unwrap_or("");
    let name = program.rsplit('/').next().unwrap_or(program);
    match name {
        // `--cmd` runs before the vimrc and `-c` after it (and after the
        // buffer is read, before any write), so a vimrc with `set undofile`
        // or `set backup` cannot turn the copies back on. Modelines are off:
        // the buffer is vault text, possibly from a shared item.
        "vi" | "vim" | "nvim" | "gvim" | "mvim" => {
            "-n -i NONE --cmd 'set noundofile nobackup nowritebackup nomodeline' \
             -c 'setlocal noundofile noswapfile' -c 'set nobackup nowritebackup'"
        }
        "emacs" | "emacsclient" => {
            "--eval '(setq make-backup-files nil auto-save-default nil create-lockfiles nil)'"
        }
        "micro" => "-backup false -savecursor false",
        _ => "",
    }
}

fn run_editor(editor: &str, path: &Path) -> Result<(), EditorError> {
    // Run through the shell so `$EDITOR` values with arguments work, exactly
    // as they do in git and Vim.
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} {} \"$1\"", hardening_args(editor)))
        .arg("sh")
        .arg(path)
        .status()
        .map_err(|e| EditorError::Failed(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(EditorError::Failed(format!("exit status {status}")))
    }
}

#[cfg(test)]
mod tests {

    /// A vimrc that turns undo files back on must not win: the hardening has
    /// to apply after the user's configuration, before anything is written.
    #[test]
    fn vim_keeps_no_undo_file_even_when_the_vimrc_enables_one() {
        if std::process::Command::new("vim").arg("--version").output().is_err() {
            eprintln!("vim is not installed; skipping");
            return;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let vimrc = dir.path().join("vimrc");
        std::fs::write(&vimrc, "set undofile\nset backup\n").unwrap();
        let buffer = dir.path().join("buffer");
        std::fs::write(&buffer, "SYNTHETIC_SECRET_123\n").unwrap();
        let script = format!(
            "vim -es -u {} {} {} -c '%s/SYNTHETIC_SECRET_123/replaced/' -c wq",
            vimrc.display(),
            hardening_args("vim"),
            buffer.display()
        );
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&buffer).unwrap().trim(), "replaced");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| name != "vimrc" && name != "buffer")
            .collect();
        assert!(leftovers.is_empty(), "vim left copies behind: {leftovers:?}");
    }
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn hardens_known_editors() {
        assert!(hardening_args("nvim").contains("-n"));
        assert!(hardening_args("/usr/bin/vim -p").contains("noundofile"));
        assert!(hardening_args("emacs -nw").contains("make-backup-files"));
        assert_eq!(hardening_args("nano"), "");
        assert_eq!(hardening_args("scripts/fake-editor"), "");
    }

    #[test]
    #[expect(unsafe_code, reason = "the test sets the environment it is reading")]
    fn resolves_in_order() {
        let previous_visual = std::env::var("VISUAL").ok();
        let previous_editor = std::env::var("EDITOR").ok();
        unsafe {
            std::env::set_var("VISUAL", "nano");
        }
        unsafe {
            std::env::set_var("EDITOR", "vim");
        }
        assert_eq!(resolve(""), "nano");
        assert_eq!(resolve("emacs"), "emacs");
        unsafe {
            std::env::remove_var("VISUAL");
        }
        assert_eq!(resolve(""), "vim");
        match previous_visual {
            Some(v) => unsafe { std::env::set_var("VISUAL", v) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
        match previous_editor {
            Some(v) => unsafe { std::env::set_var("EDITOR", v) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
    }

    #[test]
    fn edits_with_a_scripted_editor() {
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("editor.sh");
        std::fs::write(&script, "#!/bin/sh\nprintf 'edited' > \"$1\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = edit_text("original", &script.display().to_string()).unwrap();
        assert_eq!(result, "edited");
    }

    #[test]
    fn failing_editor_is_an_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("editor.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(edit_text("x", &script.display().to_string()).is_err());
    }
}
