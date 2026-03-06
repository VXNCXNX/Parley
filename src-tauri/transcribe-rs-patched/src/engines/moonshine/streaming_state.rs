use super::streaming_config::StreamingConfig;

/// Internal state for streaming inference, matching C++ `MoonshineStreamingState`.
pub struct StreamingState {
    // Frontend state
    pub sample_buffer: Vec<f32>,
    pub sample_len: i64,
    pub conv1_buffer: Vec<f32>,
    pub conv2_buffer: Vec<f32>,
    pub frame_count: i64,

    // Feature accumulator
    pub accumulated_features: Vec<f32>,
    pub accumulated_feature_count: i32,

    // Encoder tracking
    pub encoder_frames_emitted: i32,

    // Adapter position tracking
    pub adapter_pos_offset: i64,

    // Memory accumulator [T, decoder_dim]
    pub memory: Vec<f32>,
    pub memory_len: i32,

    // Decoder self-attention KV cache [depth, 1, nheads, seq_len, head_dim]
    pub k_self: Vec<f32>,
    pub v_self: Vec<f32>,
    pub cache_seq_len: i32,

    // Cross-attention KV cache [depth, 1, nheads, cross_len, head_dim]
    pub k_cross: Vec<f32>,
    pub v_cross: Vec<f32>,
    pub cross_len: i32,
    pub cross_kv_valid: bool,
}

impl StreamingState {
    /// Create a new zero-initialized streaming state for the given config.
    pub fn new(config: &StreamingConfig) -> Self {
        let mut state = StreamingState {
            sample_buffer: Vec::new(),
            sample_len: 0,
            conv1_buffer: Vec::new(),
            conv2_buffer: Vec::new(),
            frame_count: 0,
            accumulated_features: Vec::new(),
            accumulated_feature_count: 0,
            encoder_frames_emitted: 0,
            adapter_pos_offset: 0,
            memory: Vec::new(),
            memory_len: 0,
            k_self: Vec::new(),
            v_self: Vec::new(),
            cache_seq_len: 0,
            k_cross: Vec::new(),
            v_cross: Vec::new(),
            cross_len: 0,
            cross_kv_valid: false,
        };
        state.reset(config);
        state
    }

    /// Reset all state to initial values.
    pub fn reset(&mut self, config: &StreamingConfig) {
        // Frontend state
        self.sample_buffer = vec![0.0f32; 79];
        self.sample_len = 0;
        self.conv1_buffer = vec![0.0f32; config.d_model_frontend * 4];
        self.conv2_buffer = vec![0.0f32; config.c1 * 4];
        self.frame_count = 0;

        // Feature accumulator
        self.accumulated_features.clear();
        self.accumulated_feature_count = 0;

        // Encoder tracking
        self.encoder_frames_emitted = 0;

        // Adapter position
        self.adapter_pos_offset = 0;

        // Memory
        self.memory.clear();
        self.memory_len = 0;

        // Decoder cache
        self.k_self.clear();
        self.v_self.clear();
        self.cache_seq_len = 0;

        // Cross-attention KV cache
        self.k_cross.clear();
        self.v_cross.clear();
        self.cross_len = 0;
        self.cross_kv_valid = false;
    }

    /// Reset decoder self-attention KV cache only, preserving cross KV.
    pub fn decoder_reset(&mut self) {
        self.k_self.clear();
        self.v_self.clear();
        self.cache_seq_len = 0;
        // Note: cross K/V validity is preserved; it's invalidated when memory changes via encode()
    }
}
