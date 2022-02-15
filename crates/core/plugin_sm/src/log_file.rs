use std::path::Path;
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

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn buffer(&mut self) -> &mut BufWriter<File> {
        &mut self.buffer
    }
}
