// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2RequestMsg].

use crate::built_info;
use crate::client::messages::{CallService, GetAvailableEntities, GetStates};
use crate::configuration::get_driver_metadata;
use crate::controller::handler::{
    SetDriverUserDataMsg, SetupDriverMsg, SubscribeHaEventsMsg, UnsubscribeHaEventsMsg,
};
use crate::controller::{Controller, OperationModeInput, R2RequestMsg};
use crate::errors::ServiceError;
use crate::util::{return_fut_err, return_fut_ok, DeserializeMsgData};
use crate::APP_VERSION;
use actix::{fut, AsyncContext, Handler, ResponseFuture};
use lazy_static::lazy_static;
use log::{debug, error};
use serde_json::{json, Value};
use strum::EnumMessage;
use uc_api::intg::ws::{DriverVersionMsgData, R2Request};
use uc_api::intg::{EntityCommand, IntegrationVersion};
use uc_api::ws::{EventCategory, WsMessage, WsResultMsgData};

lazy_static! {
    /// Integration-API version.
    pub static ref API_VERSION: &'static str = built_info::DEPENDENCIES
        .iter()
        .find(|p| p.0 == "uc_api")
        .map(|v| v.1)
        .unwrap_or("?");
}

impl Handler<R2RequestMsg> for Controller {
    type Result = ResponseFuture<Result<Option<WsMessage>, ServiceError>>;

    fn handle(&mut self, msg: R2RequestMsg, ctx: &mut Self::Context) -> Self::Result {
        debug!("R2RequestMsg: {:?}", msg.request);
        // extra safety: if we get a request, the remote is certainly not in standby mode
        if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
            session.standby = false;
        } else {
            return_fut_err!(ServiceError::NotFound("No session found".into()));
        };

        let controller = ctx.address();
        let req_id = msg.req_id;
        let resp_msg = msg
            .request
            .get_message()
            .expect("BUG: R2Request variants must have an associated message");

        // handle metadata requests which can always be sent by the remote, no matter if the driver
        // is in "setup flow" or "running" mode
        if let Some(result) = match msg.request {
            R2Request::GetDriverVersion => Some(WsMessage::response(
                req_id,
                resp_msg,
                DriverVersionMsgData {
                    name: get_driver_metadata()
                        .ok()
                        .and_then(|drv| drv.name)
                        .and_then(|name| name.get("en").cloned()),
                    version: Some(IntegrationVersion {
                        api: Some(API_VERSION.to_string()),
                        driver: Some(APP_VERSION.to_string()),
                    }),
                },
            )),
            R2Request::GetDriverMetadata => {
                Some(WsMessage::response(req_id, resp_msg, &self.drv_metadata))
            }
            R2Request::GetDeviceState => Some(WsMessage::event(
                resp_msg,
                EventCategory::Device,
                json!({ "state": self.device_state }),
            )),
            _ => None,
        } {
            return_fut_ok!(Some(result));
        }

        // Generic acknowledge response message
        let ok = Some(WsMessage::response(req_id, resp_msg, Value::Null));

        // handle setup requests
        match msg.request {
            R2Request::SetupDriver => {
                return Box::pin(async move {
                    let setup_msg = SetupDriverMsg {
                        ws_id: msg.ws_id.clone(),
                        data: msg.deserialize()?,
                    };
                    controller.send(setup_msg).await?.map(|_| ok)
                });
            }
            R2Request::SetDriverUserData => {
                return Box::pin(async move {
                    let user_data_msg = SetDriverUserDataMsg {
                        ws_id: msg.ws_id.clone(),
                        data: msg.deserialize()?,
                    };
                    controller.send(user_data_msg).await?.map(|_| ok)
                });
            }
            _ => {}
        };

        // the remaining requests can only be handled if the driver is in the "running" mode
        if self
            .sm_consume(&msg.ws_id, &OperationModeInput::R2Request, ctx)
            .is_err()
        {
            return Box::pin(fut::result(Err(ServiceError::ServiceUnavailable(
                "Request cannot be handled: setup required".into(),
            ))));
        }

        // prepare async context
        let ha_client = self.ha_client.clone();

        // FIXME quick & dirty request id "mapping". This requires a rewrite with proper callback & timeout handling!
        let mut entitiy_ids = Default::default();
        if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
            if msg.request == R2Request::GetAvailableEntities {
                session.get_available_entities_id = Some(msg.req_id);
            } else if msg.request == R2Request::GetEntityStates {
                session.get_entity_states_id = Some(msg.req_id);
                entitiy_ids = session.subscribed_entities.clone();
            }
        }


        Box::pin(async move {
            match msg.request {
                // just for safety: include all request variants and not a catch all!
                // If we add a new request in the future, the compiler will remind us :-)
                R2Request::GetDriverVersion
                | R2Request::GetDriverMetadata
                | R2Request::GetDeviceState
                | R2Request::SetupDriver
                | R2Request::SetDriverUserData => {
                    panic!(
                        "BUG: remote request {} must have been handled by now!",
                        msg.request
                    );
                }
                R2Request::GetEntityStates => {
                    // We don't cache entities in this integration so we have to request them from HASS.
                    // I'm not aware of a different way to just retrieve the attributes. The get_states
                    // call returns everything, so we have to filter our response to UCR2.

                    // get states from Home Assistant. Response from HA will call AvailableEntities handler
                    // or call custom UC HA component command if available
                    // to get entity states on subscribed entities only
                    if let Some(ha_client) = ha_client {
                        debug!("[{}] Requesting subscribed entities states from HA {}", msg.ws_id,
                            itertools::join(entitiy_ids.clone(), ","));
                        ha_client.send(GetStates{
                            remote_id: msg.ws_id,
                            entity_ids: entitiy_ids.clone()
                        }).await??;
                        Ok(None) // asynchronous response message. TODO check if GetStates could return the response
                    } else {
                        error!(
                            "Unable to request available entities: HA client connection not available!"
                        );
                        Err(ServiceError::NotConnected)
                    }
                }
                R2Request::GetAvailableEntities => {
                    // We don't cache entities in this integration so we have to request them from HASS.
                    // I'm not aware of a different way to just retrieve the attributes. The get_states
                    // call returns everything, so we have to filter our response to UCR2.

                    // get states from Home Assistant. Response from HA will call AvailableEntities handler
                    if let Some(ha_client) = ha_client {
                        debug!("[{}] Requesting available entities from HA", msg.ws_id);
                        ha_client.send(GetAvailableEntities{
                            remote_id: msg.ws_id
                        }).await??;
                        Ok(None) // asynchronous response message. TODO check if GetStates could return the response
                    } else {
                        error!(
                            "Unable to request available entities: HA client connection not available!"
                        );
                        Err(ServiceError::NotConnected)
                    }
                }
                R2Request::SubscribeEvents => controller
                    .send(SubscribeHaEventsMsg(msg))
                    .await?
                    .map(|_| ok),
                R2Request::UnsubscribeEvents => controller
                    .send(UnsubscribeHaEventsMsg(msg))
                    .await?
                    .map(|_| ok),
                R2Request::EntityCommand => {
                    if let Some(addr) = ha_client {
                        let req_id = msg.req_id;
                        let command: EntityCommand = msg.deserialize()?;
                        match addr.send(CallService { command }).await? {
                            Err(e) => {
                                error!("CallService failed: {:?}", e);
                                Err(e)
                            }
                            Ok(_) => {
                                // plain and simple for now. We could (or better should) also wait for the HA response message...
                                let response = WsMessage::response(
                                    req_id,
                                    "result",
                                    WsResultMsgData::new("OK", "Service call sent"),
                                );
                                Ok(Some(response))
                            }
                        }
                    } else {
                        Ok(None)
                    }
                }
            }
        })
    }
}
