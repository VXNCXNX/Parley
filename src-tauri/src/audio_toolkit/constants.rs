pub const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Length of the resampled frames the recorder hands to the VAD and the feed.
pub const FRAME_MS: u64 = 30;

/// Audio kept from before the key press while the stream is open, so the first
/// word is not clipped.
pub const PRE_ROLL_MS: u64 = 450;

/// Audio kept capturing after the stop request, so the last word is not clipped.
/// Cancel skips it.
pub const POST_RELEASE_TAIL_MS: u64 = 200;
