// Copyright (c) 2022 {person OR org} <{email}>
// SPDX-License-Identifier: MPL-2.0

//! Media player entity specific HA event logic.

use log::info;

use uc_api::EntityChange;

use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn media_player_event_to_entity_change(
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    info!(
        "TODO handle media_player change event for {}: {:?}",
        data.entity_id, data.new_state.attributes
    );

    Err(ServiceError::NotYetImplemented)
}
