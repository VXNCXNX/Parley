use rubato::{FftFixedIn, Resampler};
use std::time::Duration;

const RESAMPLER_CHUNK_SIZE: usize = 1024;

pub struct FrameResampler {
    resampler: Option<FftFixedIn<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    frame_samples: usize,
    pending: Vec<f32>,
    in_hz: usize,
    out_hz: usize,
    in_count: usize,
    out_count: usize,
}

impl FrameResampler {
    pub fn new(in_hz: usize, out_hz: usize, frame_dur: Duration) -> Self {
        let frame_samples = ((out_hz as f64 * frame_dur.as_secs_f64()).round()) as usize;
        assert!(frame_samples > 0, "frame duration too short");
        let chunk_in = RESAMPLER_CHUNK_SIZE;
        let resampler = (in_hz != out_hz).then(|| {
            FftFixedIn::<f32>::new(in_hz, out_hz, chunk_in, 1, 1)
                .expect("Failed to create resampler")
        });
        Self {
            resampler,
            chunk_in,
            in_buf: Vec::with_capacity(chunk_in),
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
            in_hz,
            out_hz,
            in_count: 0,
            out_count: 0,
        }
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            self.emit_frames(src, &mut emit);
            return;
        }
        self.in_count += src.len();
        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];
            if self.in_buf.len() == self.chunk_in {
                if let Ok(out) = self
                    .resampler
                    .as_mut()
                    .unwrap()
                    .process(&[&self.in_buf[..]], None)
                {
                    self.out_count += out[0].len();
                    self.emit_frames(&out[0], &mut emit);
                }
                self.in_buf.clear();
            }
        }
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_some() && !self.in_buf.is_empty() {
            if let Ok(out) = self
                .resampler
                .as_mut()
                .unwrap()
                .process_partial(Some(&[&self.in_buf[..]]), None)
            {
                self.out_count += out[0].len();
                self.emit_frames(&out[0], &mut emit);
            }
            self.in_buf.clear();
        }
        if self.resampler.is_some() && self.in_count > 0 {
            let delay = self.resampler.as_ref().unwrap().output_delay();
            let expected = self.in_count * self.out_hz / self.in_hz + delay;
            let mut rounds = 0;
            while self.out_count < expected && rounds < 8 {
                rounds += 1;
                match self
                    .resampler
                    .as_mut()
                    .unwrap()
                    .process_partial::<&[f32]>(None, None)
                {
                    Ok(out) => {
                        let take = (expected - self.out_count).min(out[0].len());
                        self.out_count += take;
                        self.emit_frames(&out[0][..take], &mut emit);
                    }
                    Err(_) => break,
                }
            }
        }
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.in_buf.clear();
        self.pending.clear();
        self.in_count = 0;
        self.out_count = 0;
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.reset();
        }
    }

    fn emit_frames(&mut self, mut data: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !data.is_empty() {
            let space = self.frame_samples - self.pending.len();
            let take = space.min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.pending.len() == self.frame_samples {
                emit(&self.pending);
                self.pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finish_emits_resampled_tail() {
        let mut resampler = FrameResampler::new(48_000, 16_000, Duration::from_millis(30));
        let input = vec![0.2; 48_000];
        let mut count = 0;
        resampler.push(&input, |_| count += 1);
        resampler.finish(|_| count += 1);
        assert!(count >= 30, "tail missing, frames {count}");
    }
}
