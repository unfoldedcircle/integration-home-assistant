// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `GetStates` message

use std::collections::HashMap;
use std::str::FromStr;

use actix::Handler;
use awc::ws;
use log::{debug, error};
use serde_json::{json, Value};

use uc_api::{
    intg::AvailableIntgEntity, ClimateFeature, ClimateOption, CoverFeature, EntityType,
    LightFeature, MediaPlayerFeature, SensorOption,
};

use crate::client::messages::{AvailableEntities, GetStates};
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;

impl Handler<GetStates> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, _: GetStates, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] GetStates", self.id);

        let id = self.new_msg_id();
        self.entity_states_id = Some(id);
        self.send_message(
            ws::Message::Text(
                json!(
                    {"id": id, "type": "get_states"}
                )
                .to_string()
                .into(),
            ),
            "get_states",
            ctx,
        )
    }
}

impl HomeAssistantClient {
    pub(crate) fn handle_get_states_result(
        &mut self,
        entities: &[Value],
    ) -> Result<(), ServiceError> {
        let mut available = Vec::with_capacity(32);

        for entity in entities {
            let entity_id = entity
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let entity_type = match entity_id.split_once('.') {
                None => {
                    error!(
                        "[{}] Invalid entity_id format, missing dot to extract domain: {}",
                        self.id, entity_id
                    );
                    continue; // best effort
                }
                // map different entity type names
                Some((domain, _)) => match domain {
                    "input_boolean" => "switch", // TODO verify
                    v => v,
                },
            };

            let entity_type = match EntityType::from_str(entity_type) {
                Err(_) => {
                    debug!(
                        "[{}] Filtering non-supported entity: {}",
                        self.id, entity_id
                    );
                    continue;
                }
                Ok(v) => v,
            };

            if let Some(avail_entity) = create_available_entity(entity_type, entity_id, entity) {
                available.push(avail_entity);
            }
        }

        self.controller_actor.try_send(AvailableEntities {
            client_id: self.id.clone(),
            entities: available,
        })?;

        Ok(())
    }
}

// https://developers.home-assistant.io/docs/core/entity/climate#supported-features
const SUPPORT_TARGET_TEMPERATURE: u32 = 1;
const SUPPORT_TARGET_TEMPERATURE_RANGE: u32 = 2;
/* not yet used constants
const SUPPORT_TARGET_HUMIDITY: u32 = 4;
const SUPPORT_FAN_MODE: u32 = 8;
const SUPPORT_PRESET_MODE: u32 = 16;
const SUPPORT_SWING_MODE: u32 = 32;
const SUPPORT_AUX_HEAT: u32 = 64;
*/

// https://developers.home-assistant.io/docs/core/entity/media-player#supported-features
const SUPPORT_PAUSE: u32 = 1;
const SUPPORT_SEEK: u32 = 2;
const SUPPORT_VOLUME_SET: u32 = 4;
const SUPPORT_VOLUME_MUTE: u32 = 8;
const SUPPORT_PREVIOUS_TRACK: u32 = 16;
const SUPPORT_NEXT_TRACK: u32 = 32;
const SUPPORT_TURN_ON: u32 = 128;
const SUPPORT_TURN_OFF: u32 = 256;
// const SUPPORT_PLAY_MEDIA: u32 = 512;
const SUPPORT_VOLUME_STEP: u32 = 1024;
// const SUPPORT_SELECT_SOURCE: u32 = 2048;
const SUPPORT_STOP: u32 = 4096;
// const SUPPORT_CLEAR_PLAYLIST: u32 = 8192;
const SUPPORT_PLAY: u32 = 16384;
const SUPPORT_SHUFFLE_SET: u32 = 32768;
// const SUPPORT_SELECT_SOUND_MODE: u32 = 65536;
// const SUPPORT_BROWSE_MEDIA: u32 = 131072;
const SUPPORT_REPEAT_SET: u32 = 262144;
// const SUPPORT_GROUPING: u32 = 524288;

// https://developers.home-assistant.io/docs/core/entity/cover#supported-features
const COVER_SUPPORT_OPEN: u32 = 1;
const COVER_SUPPORT_CLOSE: u32 = 2;
const COVER_SUPPORT_SET_POSITION: u32 = 4;
const COVER_SUPPORT_STOP: u32 = 8;
// const COVER_SUPPORT_OPEN_TILT: u32 = 16;
// const COVER_SUPPORT_CLOSE_TILT: u32 = 32;
// const COVER_SUPPORT_STOP_TILT: u32 = 64;
// const COVER_SUPPORT_SET_TILT_POSITION: u32 = 128;

fn create_available_entity(
    entity_type: EntityType,
    entity_id: &str,
    entity: &Value,
) -> Option<AvailableIntgEntity> {
    let mut name = None;
    let mut device_class = None;
    let mut features = Vec::with_capacity(2);
    let mut options = serde_json::Map::new();

    if let Some(attributes) = entity.get("attributes").and_then(|v| v.as_object()) {
        name = attributes.get("name").and_then(|v| v.as_str());
        device_class = attributes.get("device_class").and_then(|v| v.as_str());

        let supported_features = attributes
            .get("supported_features")
            .and_then(|v| v.as_u64())
            .unwrap_or_default() as u32;

        match entity_type {
            EntityType::Button => {} // no optional features, default = "press"
            EntityType::Switch => {
                device_class = match device_class {
                    Some("outlet") | Some("switch") => device_class,
                    _ => None,
                };
                // OnOff is a default feature
                features.push("toggle".into());
            }
            EntityType::Climate => {
                let mut climate_feats = Vec::with_capacity(4);

                // FIXME this only a theoretical implementation and completely untested!
                // https://developers.home-assistant.io/docs/core/entity/climate#hvac-modes
                // Need to find some real climate devices to test...
                if let Some(hvac_modes) = attributes.get("hvac_modes").and_then(|v| v.as_array()) {
                    for hvac_mode in hvac_modes {
                        let feature = match hvac_mode.as_str().unwrap_or_default() {
                            "off" => ClimateFeature::OnOff,
                            "heat" => ClimateFeature::Heat,
                            "cool" => ClimateFeature::Cool,
                            // "fan_only" => ClimateFeature::Fan,
                            &_ => continue,
                        };
                        climate_feats.push(feature);
                    }
                }

                if supported_features & SUPPORT_TARGET_TEMPERATURE > 0 {
                    climate_feats.push(ClimateFeature::TargetTemperature);
                }
                if supported_features & SUPPORT_TARGET_TEMPERATURE_RANGE > 0 {
                    /* sorry, not yet implemented
                        climate_feats.push(ClimateFeature::TargetTemperatureRange)
                    */
                }

                // TODO is this the correct way to find out if the device can measure the current temperature?
                if is_float_value(attributes, "current_temperature") {
                    climate_feats.push(ClimateFeature::CurrentTemperature);
                }

                features = climate_feats.into_iter().map(|v| v.to_string()).collect();

                // handle options. TODO untested! Only based on some GitHub issue logs :-)
                if let Some(v) = number_value(attributes, "min_temp") {
                    options.insert(ClimateOption::MinTemperature.to_string(), v);
                }
                if let Some(v) = number_value(attributes, "max_temp") {
                    options.insert(ClimateOption::MaxTemperature.to_string(), v);
                }
                if let Some(v) = number_value(attributes, "target_temp_step") {
                    options.insert(ClimateOption::TargetTemperatureStep.to_string(), v);
                }
                // TODO how do we get the HA temperature_unit attribute? Couldn't find an example...
                if let Some(v) = attributes.get("temperature_unit") {
                    options.insert(ClimateOption::TemperatureUnit.to_string(), v.clone());
                }
            }
            EntityType::Cover => {
                device_class = match device_class {
                    Some("blind") | Some("curtain") | Some("garage") | Some("shade") => {
                        device_class
                    }
                    _ => None,
                };

                let mut cover_feats = Vec::with_capacity(2);

                if supported_features & COVER_SUPPORT_OPEN > 0 {
                    cover_feats.push(CoverFeature::Open);
                }
                if supported_features & COVER_SUPPORT_CLOSE > 0 {
                    cover_feats.push(CoverFeature::Close);
                }
                if supported_features & COVER_SUPPORT_STOP > 0 {
                    cover_feats.push(CoverFeature::Stop);
                }
                if supported_features & COVER_SUPPORT_SET_POSITION > 0 {
                    cover_feats.push(CoverFeature::Position);
                }
                features = cover_feats.into_iter().map(|v| v.to_string()).collect();
            }
            EntityType::Light => {
                let mut light_feats = Vec::with_capacity(2);
                // OnOff is default
                light_feats.push(LightFeature::Toggle);

                if let Some(color_modes) = attributes
                    .get("supported_color_modes")
                    .and_then(|v| v.as_array())
                {
                    let mut dim = false;
                    let mut color = false;
                    let mut color_temp = false;
                    for color_mode in color_modes {
                        match color_mode.as_str().unwrap_or_default() {
                            "brightness" => dim = true,
                            "color_temp" => {
                                dim = true;
                                color_temp = true;
                            }
                            "hs" | "rgb" | "rgbw" | "rgbww" | "xy" => {
                                dim = true;
                                color = true;
                            }
                            &_ => continue,
                        };
                    }
                    if dim {
                        light_feats.push(LightFeature::Dim);
                    }
                    if color {
                        light_feats.push(LightFeature::Color);
                    }
                    if color_temp {
                        light_feats.push(LightFeature::ColorTemperature);
                    }
                }
                features = light_feats.into_iter().map(|v| v.to_string()).collect();
            }
            EntityType::MediaPlayer => {
                device_class = match device_class {
                    Some("receiver") | Some("speaker") => device_class,
                    _ => None,
                };

                let mut media_feats = Vec::with_capacity(16);

                if supported_features & (SUPPORT_TURN_ON | SUPPORT_TURN_OFF) > 0 {
                    media_feats.push(MediaPlayerFeature::OnOff);
                }
                if supported_features & SUPPORT_VOLUME_SET > 0 {
                    media_feats.push(MediaPlayerFeature::Volume);
                }
                if supported_features & SUPPORT_VOLUME_STEP > 0 {
                    media_feats.push(MediaPlayerFeature::VolumeUpDown);
                }
                if supported_features & SUPPORT_VOLUME_MUTE > 0 {
                    // HASS media player doesn't support mute toggle!
                    media_feats.push(MediaPlayerFeature::Mute);
                    media_feats.push(MediaPlayerFeature::Unmute);
                }
                if supported_features & (SUPPORT_PAUSE | SUPPORT_PLAY) > 0 {
                    media_feats.push(MediaPlayerFeature::PlayPause);
                }
                if supported_features & SUPPORT_STOP > 0 {
                    media_feats.push(MediaPlayerFeature::Stop);
                }
                if supported_features & SUPPORT_NEXT_TRACK > 0 {
                    media_feats.push(MediaPlayerFeature::Next);
                }
                if supported_features & SUPPORT_PREVIOUS_TRACK > 0 {
                    media_feats.push(MediaPlayerFeature::Previous);
                }
                if supported_features & SUPPORT_REPEAT_SET > 0 {
                    media_feats.push(MediaPlayerFeature::Repeat);
                }
                if supported_features & SUPPORT_SHUFFLE_SET > 0 {
                    media_feats.push(MediaPlayerFeature::Shuffle);
                }
                if supported_features & SUPPORT_SEEK > 0 {
                    media_feats.push(MediaPlayerFeature::Seek);
                    media_feats.push(MediaPlayerFeature::MediaDuration);
                    media_feats.push(MediaPlayerFeature::MediaPosition);
                }
                media_feats.push(MediaPlayerFeature::MediaTitle);
                media_feats.push(MediaPlayerFeature::MediaArtist);
                media_feats.push(MediaPlayerFeature::MediaAlbum);
                media_feats.push(MediaPlayerFeature::MediaImageUrl);
                media_feats.push(MediaPlayerFeature::MediaType);

                /* TODO from YIO v1
                features.push("APP_NAME"); ???

                if supported_features & SUPPORT_SELECT_SOURCE > 0 {
                    features.push("SOURCE");
                }
                 */

                features = media_feats.into_iter().map(|v| v.to_string()).collect();
            }
            EntityType::Sensor => {
                device_class = match device_class {
                    Some("battery") | Some("current") | Some("energy") | Some("humidity")
                    | Some("power") | Some("temperature") | Some("voltage") => device_class,
                    // Map non-supported device classes to a custom sensor and use device class as label
                    v => {
                        if let Some(v) = v {
                            if let Some(label) = device_class_to_label(v) {
                                options.insert(
                                    SensorOption::CustomLabel.to_string(),
                                    Value::String(label),
                                );
                            }
                        }
                        if let Some(v) = attributes.get("unit_of_measurement") {
                            options.insert(SensorOption::CustomUnit.to_string(), v.clone());
                        }
                        Some("custom")
                    }
                };
            }
        }
    }

    Some(AvailableIntgEntity {
        device_id: None, // TODO prepare device_id handling
        entity_type,
        entity_id: entity_id.to_string(),
        device_class: device_class.map(|v| v.to_string()),
        name: HashMap::from([("en".to_string(), name.unwrap_or(entity_id).to_string())]),
        features: if features.is_empty() {
            None
        } else {
            Some(features)
        },
        area: None,
        options: if options.is_empty() {
            None
        } else {
            Some(options)
        },
    })
}

fn is_float_value(json: &serde_json::Map<String, Value>, key: &str) -> bool {
    json.get(key).and_then(|v| v.as_f64()).is_some()
}

fn number_value(json: &serde_json::Map<String, Value>, key: &str) -> Option<Value> {
    match json.get(key) {
        Some(v) if v.is_number() => Some(v.clone()),
        _ => None,
    }
}

fn device_class_to_label(class: &str) -> Option<String> {
    let name = class.replace('_', " ");
    let mut c = name.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
}
