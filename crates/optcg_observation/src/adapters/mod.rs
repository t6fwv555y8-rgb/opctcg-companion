pub mod browser;
pub mod desktop;
pub mod mock;
pub mod optcgsim;
pub mod replay;
pub mod screen_vision;

pub use browser::BrowserSimulatorAdapter;
pub use desktop::DesktopSimulatorAdapter;
pub use mock::MockAdapter;
pub use optcgsim::{OptcgSimAdapter, OptcgSimConfig, OptcgSimStatus};
pub use replay::ReplayAdapter;
pub use screen_vision::ScreenVisionAdapter;
