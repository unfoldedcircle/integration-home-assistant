// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use std::collections::{HashMap, HashSet};
use std::io::{Error, ErrorKind};
use std::time::Duration;

use crate::{API_VERSION, APP_VERSION};
use actix::prelude::{Actor, Context, Handler, Recipient};
use actix::{
    fut, ActorFutureExt, Addr, AsyncContext, MessageResult, ResponseActFuture, ResponseFuture,
    WrapFuture,
};
use futures::StreamExt;
use log::{debug, error, info, warn};
use serde_json::json;
use strum::EnumMessage;
use uc_api::intg::ws::{R2Event, R2Request};
use uc_api::intg::{
    ws::AvailableEntitiesMsgData, DeviceState, EntityChange, EntityCommand, IntegrationVersion,
    SubscribeEvents,
};
use uc_api::ws::{EventCategory, WsMessage, WsResultMsgData};

use crate::client::messages::{
    AvailableEntities, CallService, Close, ConnectionEvent, ConnectionState, EntityEvent, GetStates,
};
use crate::client::HomeAssistantClient;
use crate::configuration::HomeAssistantSettings;
use crate::errors::ServiceError;
use crate::from_msg_data::DeserializeMsgData;
use crate::messages::{
    Connect, Disconnect, GetDeviceState, NewR2Session, R2EventMsg, R2RequestMsg,
    R2SessionDisconnect, SendWsMessage, SubscribeHassEvents, UnsubscribeHassEvents,
};
use crate::websocket::new_websocket_client;

struct R2Session {
    recipient: Recipient<SendWsMessage>,
    standby: bool,
    subscribed_entities: HashSet<String>,
    /// HomeAssistant connection mode: true = connect (& reconnect), false = disconnect (& don't reconnect)
    ha_connect: bool,
    // TODO replace with request id map & oneshot notification
    /// quick and dirty request id mapping for get_available_entities request.
    get_available_entities_id: Option<u32>,
    /// quick and dirty request id mapping for get_entity_states request.
    get_entity_states_id: Option<u32>,
}

impl R2Session {
    fn new(recipient: Recipient<SendWsMessage>) -> Self {
        Self {
            recipient,
            standby: false,
            subscribed_entities: Default::default(),
            ha_connect: false,
            get_available_entities_id: None,
            get_entity_states_id: None,
        }
    }
}

pub struct Controller {
    // TODO use actor address instead? Recipient is generic but only allows one specific message
    /// Active Remote Two WebSocket sessions
    sessions: HashMap<String, R2Session>,
    /// Home Assistant connection state
    device_state: DeviceState,
    settings: HomeAssistantSettings,
    /// WebSocket client
    // creating an expensive client is sufficient once per process and can be used to create multiple connections
    ws_client: awc::Client,
    /// HomeAssistant client actor
    ha_client: Option<Addr<HomeAssistantClient>>,
    ha_reconnect_duration: Duration,
    ha_reconnect_attempt: u16,
}

impl Controller {
    pub fn new(settings: HomeAssistantSettings) -> Self {
        Self {
            sessions: Default::default(),
            device_state: DeviceState::Disconnected,
            ws_client: new_websocket_client(
                Duration::from_secs(settings.connection_timeout as u64),
                settings.url.scheme() == "wss",
            ),
            ha_reconnect_duration: settings.reconnect.duration,
            settings,
            ha_client: None,
            ha_reconnect_attempt: 0,
        }
    }

    /// Send a WebSocket message to the remote
    fn send_r2_msg(&self, message: WsMessage, ws_id: &str) {
        if let Some(session) = self.sessions.get(ws_id) {
            if session.standby {
                debug!("Remote is in standby, not sending message: {:?}", message);
                // TODO queue entity update events?
                return;
            }
            // TODO use send instead?
            // TODO error handling
            session.recipient.do_send(SendWsMessage(message));
        } else {
            warn!("attempting to send message but couldn't find session.");
        }
    }

    fn send_device_state(&self, ws_id: &str) {
        self.send_r2_msg(
            WsMessage::event(
                "device_state",
                EventCategory::Device,
                json!({ "state": self.device_state }),
            ),
            ws_id,
        );
    }

    fn broadcast_device_state(&self) {
        for session in self.sessions.keys() {
            // TODO filter out remotes which don't require an active HA connection?
            self.send_device_state(session);
        }
    }

    fn set_device_state(&mut self, state: DeviceState) {
        self.device_state = state;
        self.broadcast_device_state();
    }

    fn increment_reconnect_timeout(&mut self) {
        let new_timeout = Duration::from_millis(
            (self.ha_reconnect_duration.as_millis() as f32 * self.settings.reconnect.backoff_factor)
                as u64,
        );

        self.ha_reconnect_duration = if new_timeout.gt(&self.settings.reconnect.duration_max) {
            self.settings.reconnect.duration_max
        } else {
            new_timeout
        };
        info!(
            "New reconnect timeout: {}",
            self.ha_reconnect_duration.as_millis()
        )
    }
}

impl Actor for Controller {
    type Context = Context<Self>;
}

impl Handler<ConnectionEvent> for Controller {
    type Result = ();

    fn handle(&mut self, msg: ConnectionEvent, ctx: &mut Self::Context) -> Self::Result {
        match msg.state {
            ConnectionState::AuthenticationFailed => {
                // error state prevents auto-reconnect in upcoming Closed event
                self.set_device_state(DeviceState::Error);
            }
            ConnectionState::Connected => {
                self.set_device_state(DeviceState::Connected);
            }
            ConnectionState::Closed => {
                info!("HA client disconnected: {}", msg.client_id);
                self.ha_client = None;

                if matches!(
                    self.device_state,
                    DeviceState::Connecting | DeviceState::Connected
                ) {
                    info!("Start reconnecting to HA: {}", msg.client_id);
                    // TODO add incremental delay logic as in the connection establish process,
                    // otherwise there's an infinite connect -> close -> connect loop without abort
                    // for certain errors (e.g. when we forget to increment the message id).
                    self.set_device_state(DeviceState::Connecting);

                    ctx.notify(Connect {});
                }
            }
        };
    }
}

impl Handler<EntityEvent> for Controller {
    type Result = ();

    fn handle(&mut self, msg: EntityEvent, _ctx: &mut Self::Context) -> Self::Result {
        // TODO keep an entity subscription per remote session and filter out non-subscribed remotes?
        if let Ok(msg_data) = serde_json::to_value(msg.entity_change) {
            for session in self.sessions.keys() {
                self.send_r2_msg(
                    WsMessage::event("entity_change", EventCategory::Entity, msg_data.clone()),
                    session,
                );
            }
        }
    }
}

impl Handler<AvailableEntities> for Controller {
    type Result = ();

    fn handle(&mut self, msg: AvailableEntities, _ctx: &mut Self::Context) -> Self::Result {
        // TODO just a quick implementation. Implement request filter! (also caching?)
        for (ws_id, session) in self.sessions.iter_mut() {
            if session.standby {
                debug!("[{ws_id}] Remote is in standby, not handling available_entities from HASS");
                continue;
            }
            if let Some(id) = session.get_available_entities_id {
                let msg_data = AvailableEntitiesMsgData {
                    filter: None,
                    available_entities: msg.entities.clone(),
                };
                if let Ok(msg_data_json) = serde_json::to_value(msg_data) {
                    match session
                        .recipient
                        .try_send(SendWsMessage(WsMessage::response(
                            id,
                            "available_entities",
                            msg_data_json.clone(),
                        ))) {
                        Ok(_) => session.get_available_entities_id = None,
                        Err(e) => error!("[{ws_id}] Error sending available_entities: {e:?}"),
                    }
                }
            } else if let Some(id) = session.get_entity_states_id {
                let mut msg_data = Vec::with_capacity(msg.entities.len());
                for entity in &msg.entities {
                    msg_data.push(EntityChange {
                        device_id: entity.device_id.clone(),
                        entity_type: entity.entity_type.clone(),
                        entity_id: entity.entity_id.clone(),
                        attributes: entity.attributes.clone().unwrap_or_default(),
                    });
                }

                if let Ok(msg_data_json) = serde_json::to_value(msg_data) {
                    match session
                        .recipient
                        .try_send(SendWsMessage(WsMessage::response(
                            id,
                            "entity_states",
                            msg_data_json.clone(),
                        ))) {
                        Ok(_) => session.get_entity_states_id = None,
                        Err(e) => error!("[{ws_id}] Error sending entity_states: {e:?}"),
                    }
                }
            }
        }
    }
}

impl Handler<Disconnect> for Controller {
    type Result = ();

    fn handle(&mut self, _msg: Disconnect, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.ha_client.as_ref() {
            addr.do_send(Close::default());
        }
    }
}

impl Handler<Connect> for Controller {
    type Result = ResponseActFuture<Self, Result<(), Error>>;

    fn handle(&mut self, _msg: Connect, ctx: &mut Self::Context) -> Self::Result {
        // TODO check if already connected

        let ws_request = self.ws_client.ws(self.settings.url.as_str());
        // align frame size to Home Assistant
        let ws_request = ws_request.max_frame_size(self.settings.max_frame_size_kb * 1024);
        let url = self.settings.url.clone();
        let token = self.settings.token.clone();
        let client_address = ctx.address();
        let heartbeat = self.settings.heartbeat.clone();

        Box::pin(
            async move {
                debug!("Connecting to: {url}");

                let (_, framed) = match ws_request.connect().await {
                    Ok((r, f)) => (r, f),
                    Err(e) => {
                        warn!("Could not connect to {url}: {e:?}");
                        return Err(Error::new(ErrorKind::Other, e.to_string()));
                    }
                };
                info!("Connected to: {url}");

                let (sink, stream) = framed.split();
                let addr =
                    HomeAssistantClient::start(url, client_address, token, sink, stream, heartbeat);

                Ok(addr)
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |result, act, ctx| {
                match result {
                    Ok(addr) => {
                        debug!("Successfully connected to: {}", act.settings.url);
                        act.ha_client = Some(addr);
                        act.ha_reconnect_duration = act.settings.reconnect.duration;
                        act.ha_reconnect_attempt = 0;
                        Ok(())
                    }
                    Err(e) => {
                        // TODO quick and dirty: simply send Connect message as simple reconnect mechanism. Needs to be refined!
                        if act.device_state != DeviceState::Disconnected {
                            act.ha_reconnect_attempt += 1;
                            if act.ha_reconnect_attempt > act.settings.reconnect.attempts {
                                info!(
                                    "Max reconnect attempts reached ({}). Giving up!",
                                    act.settings.reconnect.attempts
                                );
                                act.device_state = DeviceState::Error;
                                act.broadcast_device_state();
                            } else {
                                ctx.notify_later(Connect {}, act.ha_reconnect_duration);
                                act.increment_reconnect_timeout();
                            }
                        }
                        Err(e)
                    }
                }
            }),
        )
    }
}

impl Handler<NewR2Session> for Controller {
    type Result = ();

    fn handle(&mut self, msg: NewR2Session, _: &mut Context<Self>) -> Self::Result {
        self.sessions
            .insert(msg.id.clone(), R2Session::new(msg.addr));

        self.send_device_state(&msg.id);
    }
}

impl Handler<R2SessionDisconnect> for Controller {
    type Result = ();

    fn handle(&mut self, msg: R2SessionDisconnect, _: &mut Context<Self>) {
        if self.sessions.remove(&msg.id).is_some() {
            // TODO
        }
    }
}

impl Handler<GetDeviceState> for Controller {
    type Result = MessageResult<GetDeviceState>;

    fn handle(&mut self, _: GetDeviceState, _: &mut Self::Context) -> Self::Result {
        MessageResult(self.device_state.clone())
    }
}

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

        let resp_msg = msg.request.get_message().unwrap();

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
                Ok(())
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
                Ok(())
            }
            R2Request::SetupDevice => Err(ServiceError::NotYetImplemented),
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
                return Box::pin(async move { addr.send(SubscribeHassEvents(msg)).await? });
            }
            R2Request::UnsubscribeEvents => {
                let addr = ctx.address();
                return Box::pin(async move { addr.send(UnsubscribeHassEvents(msg)).await? });
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
                    ctx.notify(Connect {});
                }
            }
            R2Event::Disconnect => {
                session.ha_connect = false;
                ctx.notify(Disconnect {});
                // this prevents automatic reconnects
                self.set_device_state(DeviceState::Disconnected);
            }
            R2Event::EnterStandby => {
                session.standby = true;
            }
            R2Event::ExitStandby => {
                session.standby = false;
                // TODO send updates
            }
            _ => info!("Unsupported event: {:?}", msg.event),
        }
    }
}

impl Handler<SubscribeHassEvents> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SubscribeHassEvents, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(session) = self.sessions.get_mut(&msg.0.ws_id) {
            let subscribe: SubscribeEvents = msg.0.deserialize()?;
            session
                .subscribed_entities
                .extend(subscribe.entity_ids.into_iter());
            Ok(())
        } else {
            Err(ServiceError::NotConnected)
        }
    }
}

impl Handler<UnsubscribeHassEvents> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: UnsubscribeHassEvents, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(session) = self.sessions.get_mut(&msg.0.ws_id) {
            let unsubscribe: SubscribeEvents = msg.0.deserialize()?;
            for i in unsubscribe.entity_ids {
                session.subscribed_entities.remove(&i);
            }
            Ok(())
        } else {
            Err(ServiceError::NotConnected)
        }
    }
}
