pub mod error;
pub mod file_monitor;
pub mod pipeline;
pub mod websocket_server;

pub use error::EventsError;
pub use file_monitor::{FileMonitor, FileMonitorConfig};
pub use pipeline::{EventProcessor, EventSource, InboundEvent, ProcessResult};
pub use websocket_server::{spawn_result_listener, WebSocketServer, WebSocketServerConfig};
