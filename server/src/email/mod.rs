use lettre::{
    Message, Transport,
    message::header::ContentType,
    transport::smtp::{SmtpTransport, authentication::Credentials},
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub smtp_host: String,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_name: String,
    pub from_email: String,
}

#[derive(Debug)]
pub struct SendResult {
    pub error: Option<String>,
}

pub fn send_token_email(
    config: &SmtpConfig,
    to_email: &str,
    election_title: &str,
    election_id: &str,
    token_id: &str,
    base_url: &str,
) -> SendResult {
    let token_link = format!("{}/elections/{}/tokens/{}", base_url, election_id, token_id);

    let subject = format!("Your ballot for: {}", election_title);
    let body = format!(
        "You have been invited to vote in the election \"{}\".\n\n\
         Use the link below to access your ballot:\n{}\n\n\
         This link is unique to you. Do not share it with anyone.\n\n\
         If you did not expect this invitation, please ignore this message.",
        election_title, token_link
    );

    let from_addr = format!("{} <{}>", config.from_name, config.from_email)
        .parse()
        .map_err(|e| format!("Invalid from address: {}", e));

    let from_addr = match from_addr {
        Ok(a) => a,
        Err(e) => {
            return SendResult {
                error: Some(e),
            }
        }
    };

    let to_addr = match to_email.parse() {
        Ok(a) => a,
        Err(e) => {
            return SendResult {
                error: Some(format!("Invalid to address '{}': {}", to_email, e)),
            }
        }
    };

    let email = Message::builder()
        .from(from_addr)
        .to(to_addr)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body);

    let email = match email {
        Ok(e) => e,
        Err(e) => {
            return SendResult {
                error: Some(format!("Failed to build email: {}", e)),
            }
        }
    };

    let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

    let mailer = SmtpTransport::starttls_relay(&config.smtp_host)
        .map_err(|e| format!("Invalid SMTP host: {}", e))
        .and_then(|mut builder| {
            builder = builder.credentials(creds);
            builder = builder.port(587u16);
            Ok(builder.build())
        });

    let mailer = match mailer {
        Ok(m) => m,
        Err(e) => {
            return SendResult {
                error: Some(e),
            }
        }
    };

    match mailer.send(&email) {
        Ok(_) => SendResult {
            error: None,
        },
        Err(e) => SendResult {
            error: Some(format!("SMTP send failed: {}", e)),
        },
    }
}

pub fn send_test_email(
    config: &SmtpConfig,
    to_email: &str,
    election_title: &str,
    election_id: &str,
    base_url: &str,
) -> SendResult {
    let subject = format!("{} — SMTP Test", election_title);
    let body = format!(
        "This is a test email from the STVote election system.\n\
         Your SMTP configuration is working correctly.\n\n\
         When you send ballot tokens, voters will receive an email with a link like:\n\
         {base_url}/elections/{election_id}/tokens/<TOKEN>\n\n\
         SMTP Host: {host}\n\
         From: {from_name} <{from_email}>\n\n\
         You can safely disregard this message.",
        base_url = base_url,
        election_id = election_id,
        host = config.smtp_host,
        from_name = config.from_name,
        from_email = config.from_email,
    );

    let from_addr = match format!("{} <{}>", config.from_name, config.from_email).parse() {
        Ok(a) => a,
        Err(e) => {
            return SendResult {
                error: Some(format!("Invalid from address: {}", e)),
            }
        }
    };

    let to_addr = match to_email.parse() {
        Ok(a) => a,
        Err(e) => {
            return SendResult {
                error: Some(format!("Invalid to address '{}': {}", to_email, e)),
            }
        }
    };

    let email = match Message::builder()
        .from(from_addr)
        .to(to_addr)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)
    {
        Ok(e) => e,
        Err(e) => {
            return SendResult {
                error: Some(format!("Failed to build email: {}", e)),
            }
        }
    };

    let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

    let mailer = match SmtpTransport::starttls_relay(&config.smtp_host)
        .map_err(|e| format!("Invalid SMTP host: {}", e))
        .and_then(|mut builder| {
            builder = builder.credentials(creds);
            builder = builder.port(587u16);
            Ok(builder.build())
        }) {
        Ok(m) => m,
        Err(e) => {
            return SendResult {
                error: Some(e),
            }
        }
    };

    match mailer.send(&email) {
        Ok(_) => SendResult { error: None },
        Err(e) => SendResult {
            error: Some(format!("SMTP send failed: {}", e)),
        },
    }
}

/// Parse a comma and/or newline-separated list of emails into a Vec.
/// Duplicates are removed. Returns (emails, errors) where errors lists any invalid emails.
pub fn parse_recipients(input: &str) -> (Vec<String>, Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    let mut emails = Vec::new();
    let mut errors = Vec::new();

    for part in input.split(&[',', '\n', '\r'][..]) {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = trimmed.trim_end_matches(',');
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            continue;
        }
        if !cleaned.contains('@') {
            errors.push(format!("Invalid email: {}", cleaned));
            continue;
        }
        if seen.insert(cleaned.to_string()) {
            emails.push(cleaned.to_string());
        }
    }

    (emails, errors)
}
