// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2ResponseMsg].

use crate::client::messages::SetRemoteId;
use crate::controller::{Controller, R2ResponseMsg};
use actix::Handler;
use log::{error, info};
use uc_api::intg::ws::R2Response;

impl Handler<R2ResponseMsg> for Controller {
    type Result = ();

    fn handle(&mut self, msg: R2ResponseMsg, _ctx: &mut Self::Context) -> Self::Result {
        match msg.msg {
            R2Response::RuntimeInfo => {
                info!("{:?}", msg);
            }
            R2Response::Version => {
                info!("{:?}", msg);
                if let Some(remote_id) = msg
                    .response
                    .msg_data
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .get_mut("hostname")
                    .and_then(|v| v.as_str())
                {
                    info!("Remote identifier: '{remote_id}'");
                    self.remote_id = remote_id.to_string();
                    if let Some(ha_client) = &self.ha_client
                        && let Err(e) = ha_client.try_send(SetRemoteId {
                            remote_id: self.remote_id.clone(),
                        })
                    {
                        error!("Error sending remote identifier to client: {e:?}");
                    }
                }
            }
            _ => {
                info!(
                    "[{}] TODO implement remote response: {}",
                    msg.ws_id, msg.msg
                );
            }
        }
    }
}
