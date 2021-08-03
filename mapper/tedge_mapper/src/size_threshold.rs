pub struct SizeThreshold(pub usize);

impl SizeThreshold {
    pub fn validate(&self, input: &str) -> Result<(), SizeThresholdExceeded> {
        let actual_size = input.len();
        let threshold = self.0;
        if actual_size > threshold {
            dbg!("........failed on threshold size");
            Err(SizeThresholdExceeded {
                actual_size,
                threshold,
            })
        } else {
            dbg!("........dint fail on threshold validation");
            Ok(())
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("The input size {actual_size} is too big. The threshold is {threshold}.")]
pub struct SizeThresholdExceeded {
    pub actual_size: usize,
    pub threshold: usize,
}
