use optcg_events::EventProcessor;
use std::sync::Arc;

pub struct RuntimeHandles {
    pub processor: Arc<EventProcessor>,
}
