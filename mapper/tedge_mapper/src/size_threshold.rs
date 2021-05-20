pub struct SizeThreshold(pub usize);

impl SizeThreshold {
    pub fn validate(&self, input: &[u8]) -> Result<(), SizeThresholdExceeded> {
        let actual_size = input.len();
        let threshold = self.0;
        if actual_size > threshold {
            Err(SizeThresholdExceeded {
                actual_size,
                threshold,
            })
        } else {
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
