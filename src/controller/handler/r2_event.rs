// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2EventMsg].

use crate::controller::handler::{AbortDriverSetup, ConnectMsg, DisconnectMsg};
use crate::controller::{Controller, R2EventMsg};
use actix::{AsyncContext, Handler};
use log::error;
use uc_api::intg::DeviceState;
use uc_api::intg::ws::R2Event;

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
