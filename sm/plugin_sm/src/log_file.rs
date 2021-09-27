use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::BufWriter;

pub struct LogFile {
    path: PathBuf,
    buffer: BufWriter<File>,
}

impl LogFile {
    pub async fn try_new(path: PathBuf) -> Result<LogFile, std::io::Error> {
        let file = File::create(path.clone()).await?;
        let buffer = BufWriter::new(file);

        Ok(LogFile { path, buffer })
    }

    pub fn path(&self) -> &str {
        &self.path.to_str().unwrap_or("/var/log/tedge/agent")
    }

    pub fn buffer(&mut self) -> &mut BufWriter<File> {
        &mut self.buffer
    }
}
