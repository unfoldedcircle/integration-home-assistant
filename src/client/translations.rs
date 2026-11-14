// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant state translations of sensor entities.
//!
//! The state of a HA sensor is often a technical key, e.g. `returning_home` of an `enum` sensor.
//! The HA frontend shows a translated text instead. The translations are retrieved from HA with
//! the `frontend/get_translations` request and the state is replaced in the driver: the Remote
//! doesn't know the HA integration and the translation key of an entity.

use crate::client::HomeAssistantClient;
use crate::client::messages::{CallGetTranslations, SetLanguage};
use crate::errors::ServiceError;
use actix::{Handler, ResponseActFuture};
use log::{debug, info};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use uc_api::SensorAttribute;

/// HA sensor device classes with a date or time value, which is formatted by the Remote.
const DATE_TIME_DEVICE_CLASSES: [&str; 3] = ["date", "timestamp", "uptime"];

/// HA integration and translation key of an entity, from the HA entity registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TranslationKey {
    /// HA integration providing the entity, e.g. `roborock`.
    pub platform: String,
    /// Translation key of the entity within the integration, e.g. `vacuum_error`.
    pub key: String,
}

/// State translations of the HA sensor entities in the language of the Remote.
#[derive(Debug, Default)]
pub(super) struct SensorTranslations {
    /// Map: HA entity ID -> integration and translation key
    keys: HashMap<String, TranslationKey>,
    /// Map: HA translation resource key -> translated text
    texts: HashMap<String, String>,
}

impl SensorTranslations {
    pub fn new(keys: HashMap<String, TranslationKey>, texts: HashMap<String, String>) -> Self {
        Self { keys, texts }
    }

    /// Number of translated sensor state texts.
    pub fn len(&self) -> usize {
        self.texts.len()
    }

    /// Returns the translated state of a sensor entity, `None` if there's no translation.
    ///
    /// Same lookup order as `computeStateDisplay` in the HA frontend:
    /// 1. translation of the integration, for an entity with a translation key
    /// 2. translation of the device class
    /// 3. default translation of the sensor component
    fn translate(&self, entity_id: &str, device_class: Option<&str>, state: &str) -> Option<&str> {
        let entity = self.keys.get(entity_id).map(|k| {
            format!(
                "component.{}.entity.sensor.{}.state.{state}",
                k.platform, k.key
            )
        });
        let class = device_class
            .map(|class| format!("component.sensor.entity_component.{class}.state.{state}"));
        let default = format!("component.sensor.entity_component._.state.{state}");

        entity
            .iter()
            .chain(class.iter())
            .chain(std::iter::once(&default))
            .find_map(|key| self.texts.get(key))
            .map(|text| text.as_str())
    }

    /// Replace the text value of a converted sensor entity with the translated state.
    ///
    /// The value is kept as is if there's no translation. Only text values of `sensor` entities
    /// are translated:
    /// - a numeric value is a JSON number, an unavailable or unknown value is `null`.
    /// - the `on` and `off` values of a binary sensor are translated by the Remote.
    /// - date and time values are formatted by the Remote.
    ///
    /// # Arguments
    ///
    /// * `entity_id`: HA entity id
    /// * `device_class`: HA device class of the sensor
    /// * `attributes`: converted UC sensor attributes
    pub fn apply(
        &self,
        entity_id: &str,
        device_class: Option<&str>,
        attributes: &mut Map<String, Value>,
    ) {
        if self.texts.is_empty()
            || !entity_id.starts_with("sensor.")
            || device_class.is_some_and(|class| DATE_TIME_DEVICE_CLASSES.contains(&class))
        {
            return;
        }
        let Some(Value::String(state)) = attributes.get(SensorAttribute::Value.as_ref()) else {
            return;
        };
        if let Some(text) = self.translate(entity_id, device_class, state) {
            attributes.insert(SensorAttribute::Value.to_string(), text.into());
        }
    }
}

/// Convert the language code of the Remote to a HA language code.
///
/// The Remote uses a language with an optional country code, separated by an underscore:
/// `de`, `de_CH`, `en_US`. HA uses the language only, except for a few regional variants.
/// HA returns the english texts for a language it doesn't know.
pub(super) fn ha_language(language_code: &str) -> String {
    let (language, country) = language_code
        .split_once(['_', '-'])
        .unwrap_or((language_code, ""));
    let language = language.to_lowercase();
    let country = country.to_uppercase();

    match (language.as_str(), country.as_str()) {
        ("", _) => "en",
        ("en", "GB") => "en-GB",
        ("pt", "BR") => "pt-BR",
        ("es", "419") => "es-419",
        ("zh", "TW" | "HK" | "MO") => "zh-Hant",
        ("zh", _) => "zh-Hans",
        // Norwegian Bokmål
        ("no", _) => "nb",
        (language, _) => language,
    }
    .to_string()
}

/// Parse the sensor state translations from a `frontend/get_translations` response.
///
/// The response contains all texts of the requested category: entity names, state attributes
/// etc. of all entity types. Only the state texts of sensor entities are returned.
pub(super) fn parse_sensor_state_translations(msg: &Value) -> HashMap<String, String> {
    msg.pointer("/result/resources")
        .and_then(|v| v.as_object())
        .map(|resources| {
            resources
                .iter()
                .filter(|(key, _)| is_sensor_state_key(key))
                .filter_map(|(key, text)| {
                    let text = text.as_str().filter(|text| !text.is_empty())?;
                    Some((key.clone(), text.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Returns true for the translation resource key of a sensor state:
/// - `component.<integration>.entity.sensor.<translation key>.state.<state>`
/// - `component.sensor.entity_component.<device class>.state.<state>`
fn is_sensor_state_key(key: &str) -> bool {
    let parts: Vec<&str> = key.split('.').collect();
    matches!(
        parts.as_slice(),
        ["component", _, "entity", "sensor", _, "state", _]
            | ["component", "sensor", "entity_component", _, "state", _]
    )
}

impl Handler<CallGetTranslations> for HomeAssistantClient {
    type Result = ResponseActFuture<Self, Result<HashMap<String, String>, ServiceError>>;

    fn handle(&mut self, msg: CallGetTranslations, ctx: &mut Self::Context) -> Self::Result {
        let mut request = json!({
            "type": "frontend/get_translations",
            "language": self.language,
            "category": msg.category,
        });
        if !msg.integrations.is_empty() {
            request["integration"] = msg.integrations.into();
        }

        self.request(request, "translations", ctx, |response| {
            parse_sensor_state_translations(response)
        })
    }
}

impl Handler<SetLanguage> for HomeAssistantClient {
    type Result = ();

    fn handle(&mut self, msg: SetLanguage, ctx: &mut Self::Context) -> Self::Result {
        let language = ha_language(&msg.language);
        if language == self.language {
            return;
        }
        info!(
            "[{}] Language for sensor state translations: {language}",
            self.id
        );
        self.language = language;
        if self.authenticated {
            debug!("[{}] Reloading sensor state translations", self.id);
            self.refresh_sensor_display(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn translations() -> SensorTranslations {
        SensorTranslations::new(
            HashMap::from([(
                "sensor.rocky_vacuum_error".to_string(),
                TranslationKey {
                    platform: "roborock".into(),
                    key: "vacuum_error".into(),
                },
            )]),
            HashMap::from([
                (
                    "component.roborock.entity.sensor.vacuum_error.state.lidar_blocked".to_string(),
                    "LiDAR blockiert".to_string(),
                ),
                (
                    "component.sensor.entity_component.enum.state.idle".to_string(),
                    "Leerlauf (Klasse)".to_string(),
                ),
                (
                    "component.sensor.entity_component._.state.idle".to_string(),
                    "Leerlauf".to_string(),
                ),
                (
                    "component.sensor.entity_component._.state.2026-11-15".to_string(),
                    "must not be used".to_string(),
                ),
            ]),
        )
    }

    #[rstest]
    // translation of the integration
    #[case(
        "sensor.rocky_vacuum_error",
        Some("enum"),
        "lidar_blocked",
        Some("LiDAR blockiert")
    )]
    // state without a translation in the integration: device class, then default
    #[case(
        "sensor.rocky_vacuum_error",
        Some("enum"),
        "idle",
        Some("Leerlauf (Klasse)")
    )]
    #[case("sensor.rocky_vacuum_error", None, "idle", Some("Leerlauf"))]
    // entity without a translation key
    #[case("sensor.printer", Some("enum"), "idle", Some("Leerlauf (Klasse)"))]
    #[case("sensor.printer", None, "idle", Some("Leerlauf"))]
    // no translation
    #[case("sensor.rocky_vacuum_error", Some("enum"), "new_error", None)]
    #[case("sensor.printer", None, "printing", None)]
    fn translate_uses_ha_lookup_order(
        #[case] entity_id: &str,
        #[case] device_class: Option<&str>,
        #[case] state: &str,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(
            expected,
            translations().translate(entity_id, device_class, state)
        );
    }

    #[rstest]
    #[case("sensor.rocky_vacuum_error", Some("enum"), json!("lidar_blocked"), json!("LiDAR blockiert"))]
    // without a translation the raw value is kept
    #[case("sensor.rocky_vacuum_error", Some("enum"), json!("new_error"), json!("new_error"))]
    // only text values are translated
    #[case("sensor.printer", None, json!(42), json!(42))]
    #[case("sensor.printer", None, Value::Null, Value::Null)]
    // binary sensor values are translated by the Remote
    #[case("binary_sensor.printer", None, json!("idle"), json!("idle"))]
    // date and time values are formatted by the Remote
    #[case("sensor.next_run", Some("date"), json!("2026-11-15"), json!("2026-11-15"))]
    #[case("sensor.next_run", Some("timestamp"), json!("idle"), json!("idle"))]
    #[case("sensor.next_run", Some("uptime"), json!("idle"), json!("idle"))]
    fn apply_replaces_text_value(
        #[case] entity_id: &str,
        #[case] device_class: Option<&str>,
        #[case] value: Value,
        #[case] expected: Value,
    ) {
        let mut attributes = json!({ "state": "ON", "value": value, "unit": "x" })
            .as_object()
            .cloned()
            .unwrap();

        translations().apply(entity_id, device_class, &mut attributes);

        assert_eq!(
            json!({ "state": "ON", "value": expected, "unit": "x" })
                .as_object()
                .unwrap(),
            &attributes
        );
    }

    #[test]
    fn apply_without_translations_keeps_value() {
        let mut attributes = json!({ "value": "idle" }).as_object().cloned().unwrap();

        SensorTranslations::default().apply("sensor.printer", None, &mut attributes);

        assert_eq!(Some(&json!("idle")), attributes.get("value"));
    }

    #[rstest]
    #[case("de_CH", "de")]
    #[case("de", "de")]
    #[case("en_US", "en")]
    #[case("en_GB", "en-GB")]
    #[case("en-gb", "en-GB")]
    #[case("pt_PT", "pt")]
    #[case("pt_BR", "pt-BR")]
    #[case("no_NO", "nb")]
    #[case("zh_CN", "zh-Hans")]
    #[case("zh_TW", "zh-Hant")]
    #[case("FR_fr", "fr")]
    #[case("", "en")]
    fn ha_language_from_remote_language_code(#[case] language_code: &str, #[case] expected: &str) {
        assert_eq!(expected, ha_language(language_code));
    }

    #[test]
    fn parse_sensor_state_translations_from_response() {
        let msg = json!({
            "id": 30,
            "type": "result",
            "success": true,
            "result": {
                "resources": {
                    "component.roborock.entity.sensor.vacuum_error.state.lidar_blocked": "LiDAR blockiert",
                    "component.roborock.entity.sensor.vacuum_error.name": "Staubsauger-Fehler",
                    "component.roborock.entity.sensor.status.state_attributes.mode.state.auto": "Auto",
                    "component.roborock.entity.select.mop_mode.state.deep": "Tief",
                    "component.roborock.entity.sensor.status.state.empty": "",
                    "component.roborock.entity.sensor.status.state.number": 1,
                    "component.sensor.entity_component.enum.state.idle": "Leerlauf",
                    "component.sensor.entity_component._.state.idle": "Leerlauf",
                    "component.sensor.entity_component.battery.name": "Batterie",
                    "component.binary_sensor.entity_component.door.state.on": "Offen"
                }
            }
        });

        let mut keys: Vec<_> = parse_sensor_state_translations(&msg).into_keys().collect();
        keys.sort();

        assert_eq!(
            vec![
                "component.roborock.entity.sensor.vacuum_error.state.lidar_blocked",
                "component.sensor.entity_component._.state.idle",
                "component.sensor.entity_component.enum.state.idle",
            ],
            keys
        );
    }

    #[test]
    fn parse_sensor_state_translations_without_resources() {
        assert!(parse_sensor_state_translations(&json!({ "result": {} })).is_empty());
    }
}
