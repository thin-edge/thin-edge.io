use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use reqwest::blocking;

pub struct HttpCommand {
    /// Target url
    pub url: String,

    /// HTTP request
    pub request: blocking::RequestBuilder,
}

impl Command for HttpCommand {
    fn description(&self) -> String {
        self.url.clone()
    }

    fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        Ok(self.send()?)
    }
}

impl HttpCommand {
    fn send(&self) -> Result<(), Error> {
        if let Some(request) = self.request.try_clone() {
            let http_result = request.send()?;
            let http_response = http_result.error_for_status()?;
            let bytes = http_response.bytes()?.to_vec();
            let content = String::from_utf8(bytes)?;
            println!("{content}");
        }
        Ok(())
    }
}
