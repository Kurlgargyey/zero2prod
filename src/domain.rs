use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use crate::domain::SubscriberName;
    use claims::{assert_err, assert_ok};

    #[test]
    fn a_256_grapheme_long_name_is_valid() {
        let name = "e".repeat(256);
        assert_ok!(SubscriberName::parse(name));
    }
    #[test]
    fn a_257_grapheme_long_name_is_invalid() {
        let name = "a".repeat(257);
        assert_err!(SubscriberName::parse(name));
    }
    #[test]
    fn a_whitespace_only_name_is_invalid() {
        let name = " ".into();
        assert_err!(SubscriberName::parse(name));
    }
    #[test]
    fn an_empty_name_is_invalid() {
        let name = "".into();
        assert_err!(SubscriberName::parse(name));
    }
    #[test]
    fn a_name_containing_an_invalid_character_is_invalid() {
        for name in &['/', '(', ')', '"', '<', '>', '\\', '{', '}'] {
            let name = name.to_string();
            assert_err!(SubscriberName::parse(name));
        }
    }
    #[test]
    fn a_valid_name_is_valid() {
        let name = "Ursula K. Le Guin".into();
        assert_ok!(SubscriberName::parse(name));
    }
}
