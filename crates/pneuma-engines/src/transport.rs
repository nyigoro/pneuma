use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TransportStealthProfile {
    Chrome(u32),
    Firefox(u32),
    Safari(u32),
    Edge(u32),
    Custom {
        ja3: String,
        h2_settings: Vec<u8>,
        alpn: Vec<String>,
    },
}

impl TransportStealthProfile {
    /// Parse flexible profile strings such as:
    /// - `chrome120`
    /// - `chrome_120`
    /// - `firefox123`
    /// - `safari-17`
    pub fn parse_flexible(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let normalized = trimmed.to_ascii_lowercase();

        parse_versioned(&normalized, "chrome").map(Self::Chrome).or_else(|| {
            parse_versioned(&normalized, "firefox").map(Self::Firefox)
        }).or_else(|| {
            parse_versioned(&normalized, "safari").map(Self::Safari)
        }).or_else(|| {
            parse_versioned(&normalized, "edge").map(Self::Edge)
        })
    }
}

fn parse_versioned(value: &str, prefix: &str) -> Option<u32> {
    let rest = value.strip_prefix(prefix)?;
    let rest = rest.trim_start_matches(['_', '-', ':', ' ']);
    if rest.is_empty() {
        return None;
    }
    rest.parse::<u32>().ok()
}

fn parse_version_field(value: &serde_json::Value) -> Option<u32> {
    match value {
        serde_json::Value::Number(number) => {
            number.as_u64().and_then(|v| u32::try_from(v).ok())
        }
        serde_json::Value::String(text) => text.trim().parse::<u32>().ok(),
        _ => None,
    }
}

fn parse_custom_profile(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<TransportStealthProfile> {
    let ja3 = object.get("ja3")?.as_str()?.to_string();
    let h2_settings = object
        .get("h2_settings")?
        .as_array()?
        .iter()
        .map(|value| value.as_u64().and_then(|v| u8::try_from(v).ok()))
        .collect::<Option<Vec<_>>>()?;
    let alpn = object
        .get("alpn")?
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(ToString::to_string))
        .collect::<Option<Vec<_>>>()?;

    Some(TransportStealthProfile::Custom {
        ja3,
        h2_settings,
        alpn,
    })
}

impl Serialize for TransportStealthProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match self {
            TransportStealthProfile::Chrome(version) => {
                serde_json::json!({ "type": "chrome", "version": version })
            }
            TransportStealthProfile::Firefox(version) => {
                serde_json::json!({ "type": "firefox", "version": version })
            }
            TransportStealthProfile::Safari(version) => {
                serde_json::json!({ "type": "safari", "version": version })
            }
            TransportStealthProfile::Edge(version) => {
                serde_json::json!({ "type": "edge", "version": version })
            }
            TransportStealthProfile::Custom {
                ja3,
                h2_settings,
                alpn,
            } => {
                serde_json::json!({
                    "type": "custom",
                    "ja3": ja3,
                    "h2_settings": h2_settings,
                    "alpn": alpn
                })
            }
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TransportStealthProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(text) => {
                TransportStealthProfile::parse_flexible(&text).ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        "unknown transport stealth profile string: {text}"
                    ))
                })
            }
            serde_json::Value::Object(object) => {
                let profile_type = object
                    .get("type")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_ascii_lowercase());

                match profile_type.as_deref() {
                    Some("chrome") => object
                        .get("version")
                        .and_then(parse_version_field)
                        .map(TransportStealthProfile::Chrome)
                        .ok_or_else(|| serde::de::Error::custom("chrome profile missing version")),
                    Some("firefox") => object
                        .get("version")
                        .and_then(parse_version_field)
                        .map(TransportStealthProfile::Firefox)
                        .ok_or_else(|| {
                            serde::de::Error::custom("firefox profile missing version")
                        }),
                    Some("safari") => object
                        .get("version")
                        .and_then(parse_version_field)
                        .map(TransportStealthProfile::Safari)
                        .ok_or_else(|| serde::de::Error::custom("safari profile missing version")),
                    Some("edge") => object
                        .get("version")
                        .and_then(parse_version_field)
                        .map(TransportStealthProfile::Edge)
                        .ok_or_else(|| serde::de::Error::custom("edge profile missing version")),
                    Some("chrome120") => Ok(TransportStealthProfile::Chrome(120)),
                    Some("firefox123") => Ok(TransportStealthProfile::Firefox(123)),
                    Some("safari17") => Ok(TransportStealthProfile::Safari(17)),
                    Some("custom") => parse_custom_profile(&object).ok_or_else(|| {
                        serde::de::Error::custom("custom transport profile missing fields")
                    }),
                    Some(other) => TransportStealthProfile::parse_flexible(other).ok_or_else(|| {
                        serde::de::Error::custom(format!(
                            "unknown transport profile type: {other}"
                        ))
                    }),
                    None => parse_custom_profile(&object).ok_or_else(|| {
                        serde::de::Error::custom(
                            "transport profile object must contain `type` or custom fields",
                        )
                    }),
                }
            }
            _ => Err(serde::de::Error::custom(
                "transport profile must be a string or object",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// WebDriver manual proxy value in host:port form.
    pub http_proxy: String,
    /// Optional HTTPS proxy endpoint in host:port form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_proxy: Option<String>,
    /// Optional comma-separated bypass list expressed as entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_proxy: Vec<String>,
}

impl ProxyConfig {
    pub fn ssl_or_http(&self) -> &str {
        self.ssl_proxy.as_deref().unwrap_or(&self.http_proxy)
    }
}

pub trait TransportProvider: Send + Sync {
    fn proxy_for_profile(&self, profile: &TransportStealthProfile) -> Option<ProxyConfig>;
}

#[cfg(test)]
mod tests {
    use super::TransportStealthProfile;

    #[test]
    fn parse_legacy_profile_string() {
        assert_eq!(
            TransportStealthProfile::parse_flexible("Chrome120"),
            Some(TransportStealthProfile::Chrome(120))
        );
        assert_eq!(
            TransportStealthProfile::parse_flexible("firefox_123"),
            Some(TransportStealthProfile::Firefox(123))
        );
        assert_eq!(
            TransportStealthProfile::parse_flexible("safari-17"),
            Some(TransportStealthProfile::Safari(17))
        );
    }

    #[test]
    fn deserialize_legacy_tagged_profile() {
        let profile: TransportStealthProfile =
            serde_json::from_str(r#"{"type":"chrome120"}"#).expect("profile should parse");
        assert_eq!(profile, TransportStealthProfile::Chrome(120));
    }

    #[test]
    fn deserialize_versioned_profile() {
        let profile: TransportStealthProfile =
            serde_json::from_str(r#"{"type":"firefox","version":125}"#)
                .expect("profile should parse");
        assert_eq!(profile, TransportStealthProfile::Firefox(125));
    }
}
