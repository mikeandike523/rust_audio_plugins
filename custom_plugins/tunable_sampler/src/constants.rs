pub const GUI_WIDTH: u32 = 1000;
pub const GUI_HEIGHT: u32 = 880;
pub const GUI_DEV_SERVER_URL: &str = "http://localhost:5173";
pub const GUI_PUBLISHED_URL: &str = "https://tunable-sampler-web-gui.vercel.app";
/// Default quality for offline sample-rate matching: Ultra High (2).
/// This operation runs at most once per sample, so quality wins over speed.
pub const DEFAULT_RESAMPLE_QUALITY_INPUT: u32 = 2;
/// Default quality for real-time / playback resampling: Normal (0).
pub const DEFAULT_RESAMPLE_QUALITY_PITCH: u32 = 0;
