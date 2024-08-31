// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2EventMsg].

use std::time::Duration;
use crate::controller::handler::{AbortDriverSetup, ConnectMsg, DisconnectMsg};
use crate::controller::{Controller, OperationModeInput, R2EventMsg};
use actix::{ActorFutureExt, AsyncContext, fut, Handler, ResponseActFuture, WrapFuture};
use actix::clock::sleep;
use log::{error, info};
use uc_api::intg::ws::R2Event;
use uc_api::intg::{DeviceState, DriverSetupChange};
use uc_api::model::intg::{IntegrationSetupError, IntegrationSetupState, SetupChangeEventType};
use uc_api::ws::{EventCategory, WsMessage};

impl Handler<R2EventMsg> for Controller {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: R2EventMsg, ctx: &mut Self::Context) -> Self::Result {
        let session = match self.sessions.get_mut(&msg.ws_id) {
            None => {
                error!("Session not found: {}", msg.ws_id);
                return Box::pin(fut::ready(()));
            }
            Some(s) => s,
        };

        match msg.event {
            R2Event::Connect => {
                // Connection request from remote : first check if the connection URL or token
                // have changed, whatever the connection state is
                // Ex : the user has changed the HA endpoint (from HA component),
                // the user renewed the (nearly expired) token
                if self.settings.hass.connection_settings_changed() {
                    info!("[{}] HA connection settings have changed, (re)connect to HA with the new settings... :  {}",
                        msg.ws_id, self.device_state
                    );
                    ctx.notify(ConnectMsg::default());
                    //TODO @Markus necessary to send driver_setup_change ?
                    let event = WsMessage::event(
                        "driver_setup_change",
                        EventCategory::Device,
                        serde_json::to_value(DriverSetupChange {
                            event_type: SetupChangeEventType::Stop,
                            state: IntegrationSetupState::Ok,
                            error: Option::from(IntegrationSetupError::None),
                            require_user_action: None,
                        }).expect("DriverSetupChange serialize error"),
                    );
                    session.reconfiguring = None;
                    Box::pin(
                        async move {
                            sleep(Duration::from_secs(2)).await;
                        }.into_actor(self)
                            .map(move |_, _act, _ctx| {
                                info!("(Re)connection state after configuration change {:?}", _act.device_state);
                                if _act.device_state == DeviceState::Connected {
                                    if _act
                                        .sm_consume(&msg.ws_id, &OperationModeInput::ConfigurationAvailable, _ctx)
                                        .is_err()
                                    {
                                        error!("Error during configuration, machine state {:?}", _act.machine.state());
                                    }
                                    else {
                                        info!("Machine state changed {:?}", _act.machine.state());
                                        _act.send_r2_msg(event, &msg.ws_id);
                                    }
                                }
                                _act.send_device_state(&msg.ws_id);
                            }
                        )
                    )
                }
                else if self.device_state != DeviceState::Connected {
                    // HA device not connected, retry connection
                    info!("[{}] Not connected, requesting connection to HA client :  {}",
                        msg.ws_id, self.device_state
                    );
                    ctx.notify(ConnectMsg::default());
                    self.send_device_state(&msg.ws_id);
                    return Box::pin(fut::ready(()));
                }
                else {
                    // HA device already connected and configuration unchanged, just notify state
                    self.send_device_state(&msg.ws_id);
                    return Box::pin(fut::ready(()));
                }

            }
            R2Event::Disconnect => {
                ctx.notify(DisconnectMsg {});
                return Box::pin(fut::ready(()));
            }
            R2Event::EnterStandby => {
                session.standby = true;
                if self.settings.hass.disconnect_in_standby {
                    ctx.notify(DisconnectMsg {});
                }
                return Box::pin(fut::ready(()));
            }
            R2Event::ExitStandby => {
                session.standby = false;
                if self.settings.hass.disconnect_in_standby {
                    ctx.notify(ConnectMsg::default());
                    self.send_device_state(&msg.ws_id);
                }
                return Box::pin(fut::ready(()));
            }
            R2Event::AbortDriverSetup => {
                ctx.notify(AbortDriverSetup {
                    ws_id: msg.ws_id,
                    timeout: false,
                });
                return Box::pin(fut::ready(()));
            }
        }
    }
}
