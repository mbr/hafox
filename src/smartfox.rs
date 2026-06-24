//! SmartFox HTTP client and raw XML parser.

use std::{collections::BTreeMap, str};

use quick_xml::events::{BytesStart, Event};
use reqwest::header::{ACCEPT, REFERER, USER_AGENT as USER_AGENT_HEADER};
use thiserror::Error;
use url::Url;

/// Browser-like user agent accepted by the SmartFox web endpoint.
const USER_AGENT: &str = "Mozilla/5.0 (compatible; hafox/0.1)";

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
    pub fn new(base_url: &str) -> Result<Self, Error> {
        let base_url = Url::parse(base_url).map_err(|source| Error::InvalidBaseUrl { source })?;

        Ok(Self {
            base_url,
            client: reqwest::Client::new(),
        })
    }

    /// Fetches and parses the current SmartFox values.
    pub async fn fetch_values(&self) -> Result<SmartFoxValues, Error> {
        let xml = self.fetch_values_xml().await?;
        SmartFoxValues::from_xml(&xml)
    }

    /// Fetches the raw `values.xml` payload.
    async fn fetch_values_xml(&self) -> Result<String, Error> {
        let url = self
            .base_url
            .join("values.xml")
            .map_err(|source| Error::ValuesUrl { source })?;
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

        response
            .text()
            .await
            .map_err(|source| Error::Request { source })
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
    use super::SmartFoxValues;

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
}
