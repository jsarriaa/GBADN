//! Plays the console's sound through the system's default output.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

type Queue = Arc<Mutex<VecDeque<i16>>>;

/// Sound queued beyond this is dropped, so it never lags behind the picture.
const MAX_QUEUED_SECONDS: f32 = 0.2;

/// An open sound output.
pub struct Speaker {
    queue: Queue,
    rate: u32,
    _stream: cpal::Stream,
}

impl Speaker {
    /// Opens the default output. A machine without sound gets `None`, and can still play.
    pub fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let format = supported.sample_format();
        let config = supported.config();
        let rate = config.sample_rate;
        let queue = Queue::default();

        let stream = match format {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, queue.clone()),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, queue.clone()),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, queue.clone()),
            SampleFormat::I32 => build_stream::<i32>(&device, &config, queue.clone()),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self {
            queue,
            rate,
            _stream: stream,
        })
    }

    /// The output's sample rate, which the emulated sound must match.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Queues interleaved stereo samples.
    pub fn push(&self, samples: &[i16]) {
        let limit = (self.rate as f32 * MAX_QUEUED_SECONDS) as usize * 2;
        if let Ok(mut queue) = self.queue.lock() {
            enqueue(&mut queue, samples, limit);
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Queue,
) -> Option<cpal::Stream>
where
    T: SizedSample + FromSample<i16>,
{
    let channels = config.channels as usize;
    device
        .build_output_stream(
            *config,
            move |output: &mut [T], _| {
                if let Ok(mut queue) = queue.lock() {
                    fill(&mut queue, output, channels);
                }
            },
            |_| {},
            None,
        )
        .ok()
}

fn enqueue(queue: &mut VecDeque<i16>, samples: &[i16], limit: usize) {
    if queue.len() < limit {
        queue.extend(samples);
    }
}

/// Writes queued stereo to any channel layout, with silence once the queue runs dry.
fn fill<T: Sample + FromSample<i16>>(queue: &mut VecDeque<i16>, output: &mut [T], channels: usize) {
    for frame in output.chunks_mut(channels) {
        let left = queue.pop_front().unwrap_or(0);
        let right = queue.pop_front().unwrap_or(0);
        if let [mono] = frame {
            *mono = T::from_sample(((left as i32 + right as i32) / 2) as i16);
            continue;
        }
        for (index, sample) in frame.iter_mut().enumerate() {
            let value = match index {
                0 => left,
                1 => right,
                _ => 0,
            };
            *sample = T::from_sample(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_passes_through_in_order() {
        let mut queue = VecDeque::from([1, 2, 3, 4]);
        let mut output = [0i16; 4];
        fill(&mut queue, &mut output, 2);
        assert_eq!(output, [1, 2, 3, 4]);
        assert!(queue.is_empty());
    }

    #[test]
    fn plays_silence_when_the_queue_runs_dry() {
        let mut queue = VecDeque::from([5, 6]);
        let mut output = [9i16; 4];
        fill(&mut queue, &mut output, 2);
        assert_eq!(output, [5, 6, 0, 0]);
    }

    #[test]
    fn mono_outputs_get_both_sides_mixed() {
        let mut queue = VecDeque::from([100, 200]);
        let mut output = [0i16; 1];
        fill(&mut queue, &mut output, 1);
        assert_eq!(output, [150]);
    }

    #[test]
    fn extra_channels_stay_silent() {
        let mut queue = VecDeque::from([1, 2]);
        let mut output = [9i16; 6];
        fill(&mut queue, &mut output, 6);
        assert_eq!(output, [1, 2, 0, 0, 0, 0]);
    }

    #[test]
    fn a_full_queue_drops_new_sound() {
        let mut queue = VecDeque::from([1, 2]);
        enqueue(&mut queue, &[3, 4], 2);
        assert_eq!(queue.len(), 2);
        enqueue(&mut queue, &[3, 4], 4);
        assert_eq!(queue.len(), 4);
    }

    #[test]
    fn converts_to_the_output_format() {
        let mut queue = VecDeque::from([i16::MAX, i16::MIN]);
        let mut output = [0f32; 2];
        fill(&mut queue, &mut output, 2);
        assert!(output[0] > 0.99 && output[1] < -0.99);
    }
}
