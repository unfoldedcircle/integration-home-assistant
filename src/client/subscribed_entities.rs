use actix::Handler;
use log::{debug, error};
use serde_json::json;
use crate::client::HomeAssistantClient;
use crate::client::messages::SubscribedEntities;

impl Handler<SubscribedEntities> for HomeAssistantClient {
    type Result = ();

    fn handle(&mut self, msg: SubscribedEntities, _ctx: &mut Self::Context) {
        debug!("[{}] {}", self.id, "Updated subscribed entities");
        self.subscribed_entities = msg.entity_ids;
        let id = Some(self.new_msg_id());
        if let Err(e) = self.send_json(
            json!({
                "id": id,
                "type": "uc/event/subscribed_entities",
                "result": {
                    "entities": self.subscribed_entities
                }
                }), _ctx
        ) {
            error!("[{}] Error updating subscribed entities to HA: {:?}", self.id, e);
        }
    }
}
