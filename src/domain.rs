use unicode_segmentation::UnicodeSegmentation;

pub struct SubscriberName(String);
pub struct NewSubscriber {
    pub name: SubscriberName,
    pub email: String,
}

impl SubscriberName {
    pub fn parse(input: String) -> Result<Self, String> {
        let forbidden_chars = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];

        let is_empty_or_whitespace = input.trim().is_empty();
        let is_too_long = input.graphemes(true).count() > 256;
        let contains_forbidden_chars = input.chars().any(|c| forbidden_chars.contains(&c));

        if !(is_empty_or_whitespace || is_too_long || contains_forbidden_chars) {
            Ok(Self(input.trim().to_string()))
        } else {
            Err(format!("{} is not a valid name", input))
        }
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
