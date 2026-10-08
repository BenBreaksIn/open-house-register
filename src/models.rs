use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONSENT_TEXT: &str = "I'd like the host to follow up with me about this property.";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub business_name: String,
    pub host_name: String,
    pub agent_photo_url: String,
    pub contact_email: String,
    pub agent_phone: String,
    pub agent_license: String,
    pub broker_name: String,
    pub broker_license: String,
    pub broker_phone: String,
    pub broker_email: String,
    pub logo_url: String,
    pub color: String,
    pub welcome: String,
    pub privacy_note: String,
    pub show_location: bool,
    pub show_agent_photo: bool,
    pub show_brokerage_logo: bool,
    pub ask_phone: bool,
    pub ask_timeline: bool,
    pub ask_agent: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            business_name: String::new(),
            host_name: String::new(),
            agent_photo_url: String::new(),
            contact_email: String::new(),
            agent_phone: String::new(),
            agent_license: String::new(),
            broker_name: String::new(),
            broker_license: String::new(),
            broker_phone: String::new(),
            broker_email: String::new(),
            logo_url: String::new(),
            color: "#214d3b".into(),
            welcome: "Thanks for stopping by. Make yourself at home.".into(),
            privacy_note:
                "Your details go to the host of this open house. Follow-up is your choice.".into(),
            show_location: false,
            show_agent_photo: true,
            show_brokerage_logo: true,
            ask_phone: true,
            ask_timeline: true,
            ask_agent: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OpenHouse {
    pub id: Uuid,
    pub address: String,
    pub location: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub timezone: String,
    pub photo_url: String,
    pub note: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct HouseInput {
    pub address: String,
    pub location: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub timezone: String,
    #[serde(default)]
    pub photo_url: String,
    #[serde(default)]
    pub note: String,
    pub status: String,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Visitor {
    pub id: Uuid,
    pub house_id: Uuid,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub timeline: String,
    pub represented: String,
    pub follow_up: bool,
    pub consent_text: String,
    pub checked_in_at: DateTime<Utc>,
}
#[derive(Deserialize)]
pub struct Registration {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub timeline: String,
    #[serde(default)]
    pub represented: String,
    #[serde(default)]
    pub follow_up: bool,
    #[serde(default)]
    pub website: String,
}

pub fn text(value: &str, max: usize, required: bool) -> Result<String, String> {
    let value = value.trim();
    if (required && value.is_empty())
        || value.chars().count() > max
        || value.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err(format!(
            "Please use {}–{max} characters without control characters.",
            if required { 1 } else { 0 }
        ));
    }
    Ok(value.to_owned())
}
pub fn email(value: &str) -> Result<String, String> {
    let value = value.trim().to_lowercase();
    let parts: Vec<_> = value.split('@').collect();
    if value.len() > 254
        || parts.len() != 2
        || parts[0].is_empty()
        || !parts[1].contains('.')
        || parts[1].starts_with('.')
        || parts[1].ends_with('.')
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return Err("Please enter a valid email address.".into());
    }
    Ok(value)
}
pub fn image_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value == "/assets/sample-house.png" {
        return Ok(value.into());
    }
    let url = url::Url::parse(value).map_err(|_| "Use an HTTPS image URL.")?;
    if value.len() > 2048
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an HTTPS image URL without credentials.".into());
    }
    Ok(value.into())
}
impl Settings {
    pub fn validate(mut self) -> Result<Self, String> {
        self.business_name = text(&self.business_name, 80, false)?;
        self.host_name = text(&self.host_name, 80, false)?;
        self.agent_phone = text(&self.agent_phone, 40, false)?;
        self.agent_license = text(&self.agent_license, 64, false)?;
        self.broker_name = text(&self.broker_name, 80, false)?;
        self.broker_license = text(&self.broker_license, 64, false)?;
        self.broker_phone = text(&self.broker_phone, 40, false)?;
        self.broker_email = if self.broker_email.trim().is_empty() {
            String::new()
        } else {
            email(&self.broker_email)?
        };
        self.contact_email = if self.contact_email.trim().is_empty() {
            String::new()
        } else {
            email(&self.contact_email)?
        };
        self.logo_url = image_url(&self.logo_url)?;
        self.agent_photo_url = image_url(&self.agent_photo_url)
            .map_err(|message| format!("Agent photo: {message}"))?;
        self.welcome = text(&self.welcome, 300, false)?;
        self.privacy_note = text(&self.privacy_note, 600, true)?;
        if self.color.len() != 7
            || !self.color.starts_with('#')
            || !self.color[1..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Choose a six-digit hex color.".into());
        }
        Ok(self)
    }
}
impl HouseInput {
    pub fn validate(mut self) -> Result<Self, String> {
        self.address = text(&self.address, 160, true)?;
        self.location = text(&self.location, 160, true)?;
        self.timezone = text(&self.timezone, 80, true)?;
        if self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err("Choose a valid IANA time zone.".into());
        }
        self.photo_url = image_url(&self.photo_url)?;
        self.note = text(&self.note, 1000, false)?;
        if self.ends_at <= self.starts_at
            || self.ends_at - self.starts_at > chrono::Duration::days(7)
        {
            return Err("End time must be after start time and within seven days.".into());
        }
        if !["draft", "open", "closed"].contains(&self.status.as_str()) {
            return Err("Choose draft, open, or closed.".into());
        }
        Ok(self)
    }
}
impl Registration {
    pub fn validate(mut self, settings: &Settings) -> Result<Self, String> {
        self.name = text(&self.name, 120, true)?;
        self.email = email(&self.email)?;
        self.phone = if settings.ask_phone {
            text(&self.phone, 40, false)?
        } else {
            String::new()
        };
        if !settings.ask_timeline {
            self.timeline.clear();
        }
        if !settings.ask_agent {
            self.represented.clear();
        }
        if !["", "Exploring", "0–3 months", "3–6 months", "6+ months"]
            .contains(&self.timeline.as_str())
            || !["", "Yes", "No", "Prefer not to say"].contains(&self.represented.as_str())
        {
            return Err("Choose one of the form's available answers.".into());
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_rejects_bad_urls_and_emails() {
        assert!(image_url("javascript:alert(1)").is_err());
        assert!(image_url("https://user:pass@example.com/x").is_err());
        assert!(
            Settings {
                agent_photo_url: "http://example.com/photo.png".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(email("a@b").is_err());
        assert_eq!(email(" MAYA@EXAMPLE.COM ").unwrap(), "maya@example.com");
    }
    #[test]
    fn disabled_questions_are_not_stored() {
        let settings = Settings {
            ask_phone: false,
            ask_timeline: false,
            ask_agent: false,
            ..Default::default()
        };
        let r = Registration {
            name: "Maya".into(),
            email: "maya@example.com".into(),
            phone: "secret".into(),
            timeline: "bad".into(),
            represented: "bad".into(),
            follow_up: false,
            website: String::new(),
        }
        .validate(&settings)
        .unwrap();
        assert!(r.phone.is_empty() && r.timeline.is_empty() && r.represented.is_empty());
        assert!(!r.follow_up);
    }
    #[test]
    fn color_requires_plain_hex() {
        let s = Settings {
            color: "red;url(x)".into(),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }
}
