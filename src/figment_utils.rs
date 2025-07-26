//! Helpers for using the figment config parsing library

use figment::providers::Format;
use json5 as json;

/// A figment provider that can parse JSON5.
pub struct JsonProvider;

impl Format for JsonProvider {
    type Error = json::Error;

    const NAME: &'static str = "JSON";

    fn from_str<'de, T: serde::de::DeserializeOwned>(string: &'de str) -> Result<T, Self::Error> {
        json::from_str(string)
    }
}
