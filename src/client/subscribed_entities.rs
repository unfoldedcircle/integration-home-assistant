use actix::Handler;
use log::{debug};
use crate::client::HomeAssistantClient;
use crate::client::messages::SubscribedEntities;

impl Handler<SubscribedEntities> for HomeAssistantClient {
    type Result = ();

    /// Method called by controller when subscribed entities change
    /// The custom HA component has to be updated then (if used)
    /// msg contains the new entity ids to subscribe
    fn handle(&mut self, msg: SubscribedEntities, _ctx: &mut Self::Context) {
        debug!("[{}] {} : {}", self.id, "Updated subscribed entities",
            itertools::join(&msg.entity_ids, ","));
        self.subscribed_entities = msg.entity_ids;
        if !self.authenticated {
            return;
        }
        // Occurs when the remote reloads due (wakes up) or when the user
        // selected new entities from HA, subscribe to configuration event if not already done
        self.subscribe_uc_configuration(_ctx);
        self.unsubscribe_uc_events(_ctx);
        self.subscribe_uc_events(_ctx);
    }

}
