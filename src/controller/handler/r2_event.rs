// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2EventMsg].

use crate::controller::handler::{AbortDriverSetup, ConnectMsg, DisconnectMsg};
use crate::controller::{Controller, R2EventMsg};
use actix::{AsyncContext, Handler};
use log::error;
use uc_api::intg::ws::R2Event;
use uc_api::intg::DeviceState;

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
                session.ha_connect = true;

                if self.device_state != DeviceState::Connected {
                    self.device_state = DeviceState::Connecting;
                    self.send_device_state(&msg.ws_id);
                    ctx.notify(ConnectMsg {});
                }
            }
            R2Event::Disconnect => {
                session.ha_connect = false;
                ctx.notify(DisconnectMsg {});
                // this prevents automatic reconnects
                self.set_device_state(DeviceState::Disconnected);
            }
            R2Event::EnterStandby => {
                session.standby = true;
            }
            R2Event::ExitStandby => {
                session.standby = false;
                // TODO send updates #5
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
