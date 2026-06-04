use camino::Utf8Path;
use camino::Utf8PathBuf;
use tokio::fs::File;
use tokio::io::BufWriter;

pub struct LogFile {
    path: Utf8PathBuf,
    buffer: BufWriter<File>,
}

impl LogFile {
    pub async fn try_new(path: Utf8PathBuf) -> Result<LogFile, std::io::Error> {
        let file = File::create(&path).await?;
        let buffer = BufWriter::new(file);

        Ok(LogFile { path, buffer })
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub fn buffer(&mut self) -> &mut BufWriter<File> {
        &mut self.buffer
    }
}
