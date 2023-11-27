use camino::Utf8Path;
use std::io;
use std::process;

/// Copies the file by spawning new tedge-write process.
///
/// Stdin and Stdout are UTF-8.
pub fn copy(from: &Utf8Path, to: &Utf8Path) -> io::Result<process::Output> {
    CopyOptions::new().copy(from, to)
}

/// Additional flags passed to `tedge-write` process
#[derive(Debug, PartialEq, Default)]
pub struct CopyOptions<'a> {
    /// Permission mode for the file, in octal form.
    mode: Option<u32>,

    /// User which will become the new owner of the file.
    user: Option<&'a str>,

    /// Group which will become the new owner of the file.
    group: Option<&'a str>,
}

impl<'a> CopyOptions<'a> {
    /// Create a blank new set of options ready for configuration.
    ///
    /// All the options are initially set to `None`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets new permissions for the file.
    pub fn mode(&mut self, mode: Option<u32>) -> &mut Self {
        self.mode = mode;
        self
    }

    /// Sets new owning user.
    pub fn user(&mut self, user: Option<&'a str>) -> &mut Self {
        self.user = user;
        self
    }

    /// Sets new owning group.
    pub fn group(&mut self, group: Option<&'a str>) -> &mut Self {
        self.group = group;
        self
    }

    /// Copies the file by spawning new tedge-write process.
    ///
    /// Stdin and Stdout are UTF-8.
    pub fn copy(&mut self, from: &Utf8Path, to: &Utf8Path) -> io::Result<process::Output> {
        let mut command = std::process::Command::new(crate::TEDGE_WRITE_PATH);
        let from_reader = std::fs::File::open(from)?;
        command.stdin(from_reader).arg(to);

        if let Some(mode) = self.mode {
            command.arg("--mode").arg(format!("{mode:o}"));
        }
        if let Some(user) = self.user {
            command.arg("--user").arg(user);
        }
        if let Some(group) = self.group {
            command.arg("--group").arg(group);
        }

        command.output()
    }
}
