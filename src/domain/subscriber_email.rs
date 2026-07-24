use validator::ValidateEmail;

#[derive(Debug)]
pub struct SubscriberEmail(String);

impl SubscriberEmail {
    pub fn parse(s: String) -> Result<Self, String> {
        if s.validate_email() {
            Ok(Self(s))
        } else {
            Err(format!("{} is not a valid email", s))
        }
    }
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::SubscriberEmail;
    use claims::{assert_err, assert_ok};

    #[test]
    fn an_ordinary_email_is_valid() {
        let email = "ursula@leguin.com".to_string();
        assert_ok!(SubscriberEmail::parse(email));
    }
    #[test]
    fn an_empty_email_is_invalid() {
        let email = "".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }
    #[test]
    fn an_email_without_at_sign_is_invalid() {
        let email = "mytotallyvalidmail.com".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }
    #[test]
    fn an_email_without_mailbox_identifier_is_invalid() {
        let email = "@leguin.com".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }
}
