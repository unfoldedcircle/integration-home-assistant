use crate::client::messages::SubscribedEntities;
use crate::client::HomeAssistantClient;
use actix::Handler;
use log::debug;

impl Handler<SubscribedEntities> for HomeAssistantClient {
    type Result = ();

    /// Method called by controller when subscribed entities change
    /// The custom HA component has to be updated then (if used)
    /// msg contains the new entity ids to subscribe
    fn handle(&mut self, msg: SubscribedEntities, ctx: &mut Self::Context) {
        debug!(
            "[{}] Updated subscribed entities: {:?}",
            self.id, msg.entity_ids
        );
        self.subscribed_entities = msg.entity_ids;
        if !self.authenticated {
            return;
        }
        // Occurs when the remote reloads (wakes up) or when the user
        // selected new entities from HA, subscribe to configuration event if not already done
        self.unsubscribe_uc_configuration(ctx);
        self.subscribe_uc_configuration(ctx);
        self.unsubscribe_uc_events(ctx);
        self.subscribe_uc_events(ctx);
    }
}
