// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2RequestMsg].

use crate::client::messages::{CallService, GetStates};
use crate::controller::handler::{
    SetDriverUserDataMsg, SetupDriverMsg, SubscribeHaEventsMsg, UnsubscribeHaEventsMsg,
};
use crate::controller::{Controller, OperationModeInput, R2RequestMsg};
use crate::errors::ServiceError;
use crate::util::{return_fut_err, return_fut_ok, DeserializeMsgData};
use crate::{API_VERSION, APP_VERSION};
use actix::{fut, AsyncContext, Handler, ResponseFuture};
use log::{debug, error};
use serde_json::{json, Value};
use strum::EnumMessage;
use uc_api::intg::ws::R2Request;
use uc_api::intg::{EntityCommand, IntegrationVersion};
use uc_api::ws::{EventCategory, WsMessage, WsResultMsgData};

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
                IntegrationVersion {
                    api: Some(API_VERSION.to_string()),
                    integration: Some(APP_VERSION.to_string()),
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
        if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
            if msg.request == R2Request::GetAvailableEntities {
                session.get_available_entities_id = Some(msg.req_id);
            } else if msg.request == R2Request::GetEntityStates {
                session.get_entity_states_id = Some(msg.req_id);
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
                R2Request::GetEntityStates | R2Request::GetAvailableEntities => {
                    // We don't cache entities in this integration so we have to request them from HASS.
                    // I'm not aware of a different way to just retrieve the attributes. The get_states
                    // call returns everything, so we have to filter our response to UCR2.

                    // get states from Home Assistant. Response from HA will call AvailableEntities handler
                    if let Some(ha_client) = ha_client {
                        debug!("[{}] Requesting available entities from HA", msg.ws_id);
                        ha_client.send(GetStates).await??;
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
