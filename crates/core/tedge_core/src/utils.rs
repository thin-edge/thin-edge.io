pub trait MakeCommaSeperatedString {
    fn comma_seperated(self) -> CommaSeperatedString;
}

impl MakeCommaSeperatedString for Vec<String> {
    fn comma_seperated(self) -> CommaSeperatedString {
        CommaSeperatedString(self.join(", "))
    }
}

#[derive(Debug)]
pub struct CommaSeperatedString(String);

impl std::fmt::Display for CommaSeperatedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
