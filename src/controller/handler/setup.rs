// Copyright (c) 2023 {person OR org} <{email}>
// SPDX-License-Identifier: MPL-2.0

use crate::controller::handler::{SetDriverUserDataMsg, SetupDriverMsg};
use crate::controller::{Controller, OperationModeInput};
use crate::errors::ServiceError;
use actix::Handler;
use log::debug;

impl Handler<SetupDriverMsg> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SetupDriverMsg, _ctx: &mut Self::Context) -> Self::Result {
        debug!(
            "[{}] setup driver request {}: {:?}",
            msg.ws_id, msg.req_id, msg.data
        );
        if self
            .machine
            .consume(&OperationModeInput::SetupDriverRequest)
            .is_err()
        {
            return Err(ServiceError::BadRequest(
                "Cannot start driver setup. Please abort setup first.".into(),
            ));
        }

        // just for testing
        let _ = self.machine.consume(&OperationModeInput::RequestUserInput);

        // TODO implement me #3
        Err(ServiceError::NotYetImplemented)
    }
}

impl Handler<SetDriverUserDataMsg> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SetDriverUserDataMsg, _ctx: &mut Self::Context) -> Self::Result {
        debug!(
            "[{}] set driver user data request {}: {:?}",
            msg.ws_id, msg.req_id, msg.data
        );

        if self
            .machine
            .consume(&OperationModeInput::SetupUserData)
            .is_err()
        {
            return Err(ServiceError::BadRequest(
                "Not waiting for driver user data. Please restart setup.".into(),
            ));
        }

        // TODO implement me #3
        Err(ServiceError::NotYetImplemented)
    }
}
