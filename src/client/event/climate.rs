// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific HA event logic.

use log::info;

use uc_api::EntityChange;

use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn climate_event_to_entity_change(
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    info!(
        "TODO handle climate change event for {}: {:?}",
        data.entity_id, data.new_state.attributes
    );

    Err(ServiceError::NotYetImplemented)
}
