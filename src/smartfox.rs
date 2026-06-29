//! SmartFox HTTP client and raw XML parser.

use std::{collections::BTreeMap, str};

use quick_xml::events::{BytesStart, Event};
use reqwest::header::{ACCEPT, REFERER, USER_AGENT as USER_AGENT_HEADER};
use serde::Deserialize;
use thiserror::Error;
use tracing::{debug, instrument};
use url::Url;

/// Browser-like user agent accepted by the SmartFox web endpoint.
const USER_AGENT: &str = "Mozilla/5.0 (compatible; hafox/0.1)";

/// Setting number for imported grid energy.
pub const TOTAL_ENERGY_FROM_GRID_SETTING: &str = "41000";
/// Setting number for exported grid energy.
pub const TOTAL_ENERGY_TO_GRID_SETTING: &str = "41004";
/// Setting number for SmartFox energy.
pub const TOTAL_ENERGY_SMARTFOX_SETTING: &str = "41008";

/// Setting number for the user password hash.
const USER_PASSWORD_SETTING: &str = "44300";
/// Setting number for the installer password hash.
const INSTALLER_PASSWORD_SETTING: &str = "44319";
/// Setting number for the SmartFox server address.
const NETWORK_SERVER_IP_SETTING: &str = "42008";
/// Setting number for the SmartFox upload cycle.
const NETWORK_UPLOAD_CYCLE_SETTING: &str = "42010";
/// Setting number for login activation.
const LOGIN_ENABLED_SETTING: &str = "44317";
/// Setting number for automatic restart activation.
const ADMINISTRATION_DAILY_RESTART_SETTING: &str = "44500";

/// Settings value identifier for imported grid energy.
const TOTAL_ENERGY_FROM_GRID_ID: &str = "energyFromGrid";
/// Settings value identifier for exported grid energy.
const TOTAL_ENERGY_TO_GRID_ID: &str = "energyToGrid";
/// Settings value identifier for SmartFox energy.
const TOTAL_ENERGY_SMARTFOX_ID: &str = "energySmartfox";
/// Settings value identifier for the user password hash.
const USER_PASSWORD_ID: &str = "userPassword";
/// Settings value identifier for the installer password hash.
const INSTALLER_PASSWORD_ID: &str = "installerPassword";
/// Settings value identifier for the SmartFox server address.
const NETWORK_SERVER_IP_ID: &str = "networkServerIp";
/// Settings value identifier for the SmartFox upload cycle.
const NETWORK_UPLOAD_CYCLE_ID: &str = "networkUploadCycle";
/// Settings value identifier for login activation.
const LOGIN_ENABLED_ID: &str = "hidloginEnabled";
/// Settings value identifier for automatic restart activation.
const ADMINISTRATION_DAILY_RESTART_ID: &str = "hidAdministrationDailyRestart";

/// Describes SmartFox settings to update.
#[derive(Clone, Debug, Default)]
pub struct SettingsUpdate {
    /// Imported grid energy in watt-hours.
    pub total_energy_from_grid: Option<u64>,
    /// Exported grid energy in watt-hours.
    pub total_energy_to_grid: Option<u64>,
    /// SmartFox energy in watt-hours.
    pub total_energy_smartfox: Option<u64>,
}

impl SettingsUpdate {
    /// Submits the settings update to a SmartFox device.
    #[instrument(skip_all)]
    pub async fn submit(&self, client: &SmartFoxClient) -> Result<(), Error> {
        client.submit_settings_update(self).await
    }
}

/// Retrieves SmartFox measurements from the local web endpoint.
#[derive(Clone, Debug)]
pub struct SmartFoxClient {
    /// Base URL of the SmartFox web interface.
    base_url: Url,
    /// HTTP client used for SmartFox requests.
    client: reqwest::Client,
}

impl SmartFoxClient {
    /// Creates a client from a SmartFox base URL.
    #[instrument(skip_all, fields(base_url = %base_url))]
    pub fn new(base_url: &str) -> Result<Self, Error> {
        let base_url = Url::parse(base_url).map_err(|source| Error::InvalidBaseUrl { source })?;
        debug!(%base_url, "created SmartFox client");

        Ok(Self {
            base_url,
            client: reqwest::Client::new(),
        })
    }

    /// Fetches and parses the current SmartFox values.
    #[instrument(skip(self))]
    pub async fn fetch_values(&self) -> Result<SmartFoxValues, Error> {
        let xml = self.fetch_values_xml().await?;
        SmartFoxValues::from_xml(&xml)
    }

    /// Fetches the raw `values.xml` payload.
    #[instrument(skip(self))]
    async fn fetch_values_xml(&self) -> Result<String, Error> {
        self.fetch_xml("values.xml").await
    }

    /// Fetches and parses the SmartFox settings values.
    #[instrument(skip(self))]
    async fn fetch_settings_values(&self) -> Result<SmartFoxValues, Error> {
        let xml = self.fetch_xml("settings_values.xml").await?;
        SmartFoxValues::from_xml(&xml)
    }

    /// Submits a settings update to the SmartFox web endpoint.
    #[instrument(skip_all)]
    async fn submit_settings_update(&self, update: &SettingsUpdate) -> Result<(), Error> {
        let settings = self.fetch_settings_values().await?;
        let request = settings_update_request(&settings, update)?;
        let url = self
            .base_url
            .join("setallg.cgi")
            .map_err(|source| Error::SettingsUrl { source })?;
        debug!(%url, "submitting SmartFox settings update");
        let response = self
            .client
            .get(url)
            .query(&request)
            .header(USER_AGENT_HEADER, USER_AGENT)
            .header(ACCEPT, "*/*")
            .header(REFERER, self.base_url.as_str())
            .send()
            .await
            .map_err(|source| Error::Request { source })?
            .error_for_status()
            .map_err(|source| Error::Request { source })?;
        let body = response
            .text()
            .await
            .map_err(|source| Error::Request { source })?;
        let response: SettingsSubmitResponse =
            serde_json::from_str(&body).map_err(|source| Error::SettingsResponse { source })?;
        if response.status != "true" {
            return Err(Error::SettingsRejected {
                status: response.status,
            });
        }

        Ok(())
    }

    /// Fetches a SmartFox XML endpoint.
    #[instrument(skip(self))]
    async fn fetch_xml(&self, endpoint: &str) -> Result<String, Error> {
        let url = self
            .base_url
            .join(endpoint)
            .map_err(|source| Error::ValuesUrl { source })?;
        debug!(%url, "fetching SmartFox XML");
        let response = self
            .client
            .get(url)
            .header(USER_AGENT_HEADER, USER_AGENT)
            .header(ACCEPT, "*/*")
            .header(REFERER, self.base_url.as_str())
            .send()
            .await
            .map_err(|source| Error::Request { source })?
            .error_for_status()
            .map_err(|source| Error::Request { source })?;

        let xml = response
            .text()
            .await
            .map_err(|source| Error::Request { source })?;
        debug!(bytes = xml.len(), "fetched SmartFox XML");

        Ok(xml)
    }
}

/// Stores raw SmartFox values keyed by their XML identifier.
#[derive(Clone, Debug, PartialEq)]
pub struct SmartFoxValues {
    /// XML values indexed by SmartFox identifier.
    entries: BTreeMap<String, String>,
}

impl SmartFoxValues {
    /// Parses a SmartFox `values.xml` payload.
    #[instrument(skip_all, fields(bytes = xml.len()))]
    pub fn from_xml(xml: &str) -> Result<Self, Error> {
        let mut reader = quick_xml::Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut entries = BTreeMap::new();
        let mut current_id = None;
        let mut current_text = String::new();

        loop {
            match reader
                .read_event()
                .map_err(|source| Error::Xml { source })?
            {
                Event::Start(element) if element.name().as_ref() == b"value" => {
                    current_id = value_id(&element)?;
                    current_text.clear();
                }
                Event::Text(text) if current_id.is_some() => {
                    let value =
                        str::from_utf8(text.as_ref()).map_err(|source| Error::Utf8 { source })?;
                    current_text.push_str(value);
                }
                Event::GeneralRef(reference) if current_id.is_some() => {
                    let reference = str::from_utf8(reference.as_ref())
                        .map_err(|source| Error::Utf8 { source })?;
                    current_text.push('&');
                    current_text.push_str(reference);
                    current_text.push(';');
                }
                Event::End(element) if element.name().as_ref() == b"value" => {
                    if let Some(id) = current_id.take() {
                        entries.insert(id, normalize_text(&current_text));
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }

        debug!(values = entries.len(), "parsed SmartFox XML");

        Ok(Self { entries })
    }

    /// Returns the raw value for a SmartFox key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// Builds raw values from key-value pairs.
    #[cfg(test)]
    pub fn from_pairs<const N: usize>(pairs: [(&str, &str); N]) -> Self {
        let entries = pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();

        Self { entries }
    }
}

/// Describes a SmartFox settings submission response.
#[derive(Debug, Deserialize)]
struct SettingsSubmitResponse {
    /// Whether the settings update was accepted.
    status: String,
}

/// Builds the `setallg.cgi` settings request.
fn settings_update_request(
    settings: &SmartFoxValues,
    update: &SettingsUpdate,
) -> Result<Vec<(&'static str, String)>, Error> {
    Ok(vec![
        (
            USER_PASSWORD_SETTING,
            required_setting(settings, USER_PASSWORD_ID)?,
        ),
        (
            INSTALLER_PASSWORD_SETTING,
            required_setting(settings, INSTALLER_PASSWORD_ID)?,
        ),
        (
            NETWORK_SERVER_IP_SETTING,
            required_setting(settings, NETWORK_SERVER_IP_ID)?,
        ),
        (
            NETWORK_UPLOAD_CYCLE_SETTING,
            required_setting(settings, NETWORK_UPLOAD_CYCLE_ID)?,
        ),
        (
            TOTAL_ENERGY_FROM_GRID_SETTING,
            updated_energy_setting(
                settings,
                TOTAL_ENERGY_FROM_GRID_ID,
                update.total_energy_from_grid,
            )?,
        ),
        (
            TOTAL_ENERGY_TO_GRID_SETTING,
            updated_energy_setting(
                settings,
                TOTAL_ENERGY_TO_GRID_ID,
                update.total_energy_to_grid,
            )?,
        ),
        (
            TOTAL_ENERGY_SMARTFOX_SETTING,
            updated_energy_setting(
                settings,
                TOTAL_ENERGY_SMARTFOX_ID,
                update.total_energy_smartfox,
            )?,
        ),
        (
            LOGIN_ENABLED_SETTING,
            required_setting(settings, LOGIN_ENABLED_ID)?,
        ),
        (
            ADMINISTRATION_DAILY_RESTART_SETTING,
            required_setting(settings, ADMINISTRATION_DAILY_RESTART_ID)?,
        ),
    ])
}

/// Returns a required SmartFox setting value.
fn required_setting(settings: &SmartFoxValues, field: &'static str) -> Result<String, Error> {
    settings
        .get(field)
        .map(ToOwned::to_owned)
        .ok_or(Error::MissingSetting { field })
}

/// Returns either an updated or existing energy setting value.
fn updated_energy_setting(
    settings: &SmartFoxValues,
    field: &'static str,
    update: Option<u64>,
) -> Result<String, Error> {
    match update {
        Some(value) => Ok(value.to_string()),
        None => required_setting(settings, field),
    }
}

/// Reports SmartFox retrieval and parsing failures.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that the configured base URL is invalid.
    #[error("SmartFox base URL is invalid")]
    InvalidBaseUrl {
        /// URL parser error.
        #[source]
        source: url::ParseError,
    },
    /// Indicates that the `values.xml` URL could not be built.
    #[error("SmartFox values URL could not be built")]
    ValuesUrl {
        /// URL join error.
        #[source]
        source: url::ParseError,
    },
    /// Indicates that the settings submission URL could not be built.
    #[error("SmartFox settings URL could not be built")]
    SettingsUrl {
        /// URL join error.
        #[source]
        source: url::ParseError,
    },
    /// Indicates that a required setting was missing.
    #[error("SmartFox setting `{field}` is missing")]
    MissingSetting {
        /// Missing SmartFox setting name.
        field: &'static str,
    },
    /// Indicates that the settings submission response was malformed.
    #[error("SmartFox settings response could not be parsed")]
    SettingsResponse {
        /// JSON parser error.
        #[source]
        source: serde_json::Error,
    },
    /// Indicates that the settings submission was rejected.
    #[error("SmartFox settings update was rejected with status `{status}`")]
    SettingsRejected {
        /// Rejection status reported by the device.
        status: String,
    },
    /// Indicates that the HTTP request failed.
    #[error("SmartFox request failed")]
    Request {
        /// HTTP client error.
        #[source]
        source: reqwest::Error,
    },
    /// Indicates that the XML payload could not be parsed.
    #[error("SmartFox XML could not be parsed")]
    Xml {
        /// XML parser error.
        #[source]
        source: quick_xml::Error,
    },
    /// Indicates that a value attribute could not be parsed.
    #[error("SmartFox XML attribute could not be parsed")]
    Attribute {
        /// XML attribute parser error.
        #[source]
        source: quick_xml::events::attributes::AttrError,
    },
    /// Indicates that XML text was not valid UTF-8.
    #[error("SmartFox XML text is not valid UTF-8")]
    Utf8 {
        /// UTF-8 parser error.
        #[source]
        source: str::Utf8Error,
    },
}

/// Extracts the SmartFox identifier from a value element.
fn value_id(element: &BytesStart<'_>) -> Result<Option<String>, Error> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|source| Error::Attribute { source })?;
        if attribute.key.as_ref() == b"id" {
            let value = str::from_utf8(attribute.value.as_ref())
                .map_err(|source| Error::Utf8 { source })?;
            return Ok(Some(value.to_owned()));
        }
    }

    Ok(None)
}

/// Normalizes text produced by the SmartFox XML endpoint.
fn normalize_text(value: &str) -> String {
    value
        .trim()
        .replace("&lt;span&gt;", " ")
        .replace("&lt;/span&gt;", "")
        .replace("<span>", " ")
        .replace("</span>", "")
        .replace("&#176;", "°")
        .replace("&#x25;", "%")
        .replace("Â°C", "°C")
}

#[cfg(test)]
mod tests {
    use super::{SettingsUpdate, SmartFoxValues, settings_update_request};

    /// Parses keyed values from SmartFox XML.
    #[test]
    fn parses_values_xml() {
        let values = SmartFoxValues::from_xml(
            r#"<root><value id="hidPower">34 W</value><value id="analogOutPower">0.00 &lt;span&gt;kW&lt;/span&gt;</value><value id="batteryTemp">31&#176;C</value><value id="batterySoc">32%</value></root>"#,
        )
        .expect("XML should parse");

        assert_eq!(values.get("hidPower"), Some("34 W"));
        assert_eq!(values.get("analogOutPower"), Some("0.00 kW"));
        assert_eq!(values.get("batteryTemp"), Some("31°C"));
        assert_eq!(values.get("batterySoc"), Some("32%"));
    }

    /// Builds settings requests while preserving untouched values.
    #[test]
    fn builds_settings_update_request() {
        let values = SmartFoxValues::from_pairs([
            ("userPassword", "1509442"),
            ("installerPassword", ""),
            ("networkServerIp", "93.189.25.182"),
            ("networkUploadCycle", "15"),
            ("energyFromGrid", "3047978"),
            ("energyToGrid", "5351925"),
            ("energySmartfox", "0"),
            ("hidloginEnabled", "0"),
            ("hidAdministrationDailyRestart", "1"),
        ]);
        let update = SettingsUpdate {
            total_energy_from_grid: Some(3_047_900),
            total_energy_to_grid: None,
            total_energy_smartfox: None,
        };

        let request = settings_update_request(&values, &update).expect("request should build");

        assert_eq!(request[4], ("41000", "3047900".to_owned()));
        assert_eq!(request[5], ("41004", "5351925".to_owned()));
    }
}
