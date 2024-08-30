// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2EventMsg].

use crate::controller::handler::{AbortDriverSetup, ConnectMsg, DisconnectMsg};
use crate::controller::{Controller, R2EventMsg, SendWsMessage};
use actix::{AsyncContext, Handler};
use log::{error, info};
use uc_api::intg::ws::R2Event;
use uc_api::intg::DeviceState;
use uc_api::ws::WsMessage;

impl Handler<R2EventMsg> for Controller {
    type Result = ();

    fn handle(&mut self, msg: R2EventMsg, ctx: &mut Self::Context) -> Self::Result {
        let session = match self.sessions.get_mut(&msg.ws_id) {
            None => {
                error!("Session not found: {}", msg.ws_id);
                return;
            }
            Some(s) => s,
        };

        match msg.event {
            R2Event::Connect => {
                if self.device_state != DeviceState::Connected {
                    info!(
                        "[{}] Not connected, requesting registered HA tokens from remote:  {}",
                        msg.ws_id, self.device_state
                    );
                    let fakeid = 32768;
                    //TODO @Markus this message type should be an EVENT and not a REQUEST :
                    // WS connection R2 => HA driver : R2 = server, HA driver = client
                    // Meantime I have built the message from a json object which is not clean
                    // With event type we will be able to remove the fakeid
                    let json = serde_json::json!({
                        "kind": "req",
                        "id": Some(fakeid),
                        "msg": "get_runtime_info",
                    });
                    let request: WsMessage =
                        serde_json::from_value(json).expect("Invalid json message");

                    match session.recipient.try_send(SendWsMessage(request)) {
                        Ok(_) => info!("[{}] Request sent", fakeid),
                        Err(e) => error!("[{}] Error sending entity_states: {e:?}", msg.ws_id),
                    }

                    ctx.notify(ConnectMsg::default());
                }
                // make sure client has the correct state, it might be out of sync, or not calling get_device_state
                self.send_device_state(&msg.ws_id);
            }
            R2Event::Disconnect => {
                ctx.notify(DisconnectMsg {});
            }
            R2Event::EnterStandby => {
                session.standby = true;
                if self.settings.hass.disconnect_in_standby {
                    ctx.notify(DisconnectMsg {});
                }
            }
            R2Event::ExitStandby => {
                session.standby = false;
                if self.settings.hass.disconnect_in_standby {
                    ctx.notify(ConnectMsg::default());
                    self.send_device_state(&msg.ws_id);
                }
            }
            R2Event::AbortDriverSetup => {
                ctx.notify(AbortDriverSetup {
                    ws_id: msg.ws_id,
                    timeout: false,
                });
            }
        }
    }
}
