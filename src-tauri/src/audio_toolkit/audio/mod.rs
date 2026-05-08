// Re-export all audio components
mod device;
#[cfg(target_os = "macos")]
pub mod macos_audio;
mod recorder;
mod resampler;
mod utils;
mod visualizer;

pub use device::{list_input_devices, list_output_devices, CpalDeviceInfo};
pub use recorder::AudioRecorder;
pub use resampler::FrameResampler;
pub use utils::{load_wav_file, save_wav_file};
pub use visualizer::AudioVisualiser;
