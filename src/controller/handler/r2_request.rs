// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2RequestMsg].

use crate::client::messages::{CallService, GetStates};
use crate::controller::handler::{
    SetDriverUserDataMsg, SetupDriverMsg, SubscribeHaEventsMsg, UnsubscribeHaEventsMsg,
};
use crate::controller::{Controller, OperationModeInput, R2RequestMsg, SendWsMessage};
use crate::errors::ServiceError;
use crate::util::DeserializeMsgData;
use crate::{API_VERSION, APP_VERSION};
use actix::{fut, AsyncContext, Handler, ResponseFuture};
use log::{debug, error};
use serde_json::json;
use strum::EnumMessage;
use uc_api::intg::ws::R2Request;
use uc_api::intg::{EntityCommand, IntegrationVersion};
use uc_api::ws::{EventCategory, WsMessage, WsResultMsgData};

impl Handler<R2RequestMsg> for Controller {
    type Result = ResponseFuture<Result<(), ServiceError>>;

    fn handle(&mut self, msg: R2RequestMsg, ctx: &mut Self::Context) -> Self::Result {
        debug!("R2RequestMsg: {:?}", msg.request);
        // extra safety: if we get a request, the remote is certainly not in standby mode
        let r2_recipient = if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
            session.standby = false;
            session.recipient.clone()
        } else {
            error!("Can't handle R2RequestMsg without a session!");
            return Box::pin(fut::result(Ok(())));
        };

        let resp_msg = msg
            .request
            .get_message()
            .expect("R2Request variants must have an associated message");

        // handle metadata requests which can always be sent by the remote, no matter if the driver
        // is in "setup flow" or "running" mode
        let result = match msg.request {
            R2Request::GetDriverVersion => {
                self.send_r2_msg(
                    WsMessage::response(
                        msg.req_id,
                        resp_msg,
                        IntegrationVersion {
                            api: API_VERSION.to_string(),
                            integration: APP_VERSION.to_string(),
                        },
                    ),
                    &msg.ws_id,
                );
                Some(Ok(()))
            }
            R2Request::GetDriverMetadata => {
                self.send_r2_msg(
                    WsMessage::response(msg.req_id, resp_msg, &self.drv_metadata),
                    &msg.ws_id,
                );
                Some(Ok(()))
            }
            R2Request::GetDeviceState => {
                self.send_r2_msg(
                    WsMessage::event(
                        resp_msg,
                        EventCategory::Device,
                        json!({ "state": self.device_state }),
                    ),
                    &msg.ws_id,
                );
                Some(Ok(()))
            }
            _ => None,
        };

        if let Some(result) = result {
            return Box::pin(fut::result(result));
        }

        // handle setup requests
        match msg.request {
            R2Request::SetupDriver => {
                let addr = ctx.address();
                return Box::pin(async move {
                    let setup_msg = SetupDriverMsg {
                        ws_id: msg.ws_id.clone(),
                        req_id: msg.req_id,
                        data: msg.deserialize()?,
                    };
                    addr.send(setup_msg).await?
                });
            }
            R2Request::SetDriverUserData => {
                let addr = ctx.address();
                return Box::pin(async move {
                    let setup_msg = SetDriverUserDataMsg {
                        ws_id: msg.ws_id.clone(),
                        req_id: msg.req_id,
                        data: msg.deserialize()?,
                    };
                    addr.send(setup_msg).await?
                });
            }
            _ => {}
        };

        // the remaining requests can only be handled if the driver is in the "running" mode
        if self
            .machine
            .consume(&OperationModeInput::R2Request)
            .is_err()
        {
            return Box::pin(fut::result(Err(ServiceError::ServiceUnavailable(
                "Request cannot be handled: setup required".into(),
            ))));
        }

        let result = match msg.request {
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
                // TODO quick & dirty request id "mapping"
                if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
                    if msg.request == R2Request::GetAvailableEntities {
                        session.get_available_entities_id = Some(msg.req_id);
                    } else {
                        session.get_entity_states_id = Some(msg.req_id);
                    }
                }

                // get states from HASS. Response will call AvailableEntities handler
                if let Some(addr) = self.ha_client.as_ref() {
                    debug!("[{}] Requesting available entities from HA", msg.ws_id);
                    addr.do_send(GetStates);
                    Ok(())
                } else {
                    error!(
                        "Unable to request available entities: HA client connection not available!"
                    );
                    Err(ServiceError::NotConnected)
                }
            }
            R2Request::SubscribeEvents => {
                let addr = ctx.address();
                return Box::pin(async move { addr.send(SubscribeHaEventsMsg(msg)).await? });
            }
            R2Request::UnsubscribeEvents => {
                let addr = ctx.address();
                return Box::pin(async move { addr.send(UnsubscribeHaEventsMsg(msg)).await? });
            }
            R2Request::EntityCommand => {
                if let Some(addr) = self.ha_client.clone() {
                    return Box::pin(async move {
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
                                if let Err(e) = r2_recipient.try_send(SendWsMessage(response)) {
                                    error!("Can't send R2 result: {e}");
                                }
                                Ok(())
                            }
                        }
                    });
                };
                Ok(())
            }
        };

        Box::pin(fut::result(result))
    }
}
