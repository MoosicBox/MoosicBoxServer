use std::sync::RwLock;

use crate::output::{to_samples, AudioEncoder, AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::play_file_path_str;
use crate::resampler::Resampler;

use bytes::Bytes;
use moosicbox_converter::aac::encoder_aac;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::*;
use symphonia::core::units::Duration;

pub struct AacEncoder {
    resampler: Option<RwLock<Resampler<i16>>>,
    input_rate: Option<u32>,
    resample_rate: Option<u32>,
    output_rate: usize,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: fdk_aac::enc::Encoder,
}

impl AacEncoder {
    pub fn new() -> Self {
        Self {
            resampler: None,
            input_rate: None,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: None,
            encoder: encoder_aac().unwrap(),
        }
    }

    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: None,
            input_rate: None,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: Some(Box::new(writer)),
            encoder: encoder_aac().unwrap(),
        }
    }

    pub fn init_resampler(&mut self, spec: &SignalSpec, duration: Duration) -> &Self {
        self.input_rate.replace(spec.rate);
        self.duration.replace(duration);

        if !self.resample_rate.is_some_and(|r| r == spec.rate)
            && self.output_rate != spec.rate as usize
        {
            log::debug!(
                "Initializing resampler with rate={} duration={}",
                spec.rate,
                duration,
            );
            self.resample_rate.replace(spec.rate);
            self.resampler.replace(RwLock::new(Resampler::new(
                *spec,
                self.output_rate,
                duration,
            )));
        }
        self
    }

    pub fn open(mut self, spec: SignalSpec, duration: Duration) -> Self {
        self.init_resampler(&spec, duration);
        self
    }

    fn encode_output(&self, buf: &[i16]) -> Bytes {
        let mut read = 0;
        let mut written = vec![];
        loop {
            let end = std::cmp::min(read + 1024, buf.len());
            let mut output = [0u8; 2048];
            match moosicbox_converter::aac::encode_aac(&self.encoder, &buf[read..end], &mut output)
            {
                Ok(info) => {
                    written.extend_from_slice(&output[..info.output_size]);
                    read += info.input_consumed;

                    if read >= buf.len() {
                        break;
                    }
                }
                Err(err) => {
                    panic!("Failed to convert: {err:?}");
                }
            }
        }
        written.into()
    }

    fn resample_if_needed(
        &mut self,
        decoded: AudioBuffer<f32>,
    ) -> Result<Vec<i16>, AudioOutputError> {
        let spec = decoded.spec();
        let duration = decoded.capacity() as u64;

        self.init_resampler(spec, duration);

        if let Some(resampler) = &self.resampler {
            log::debug!(
                "Resampling input_rate={:?} output_rate={} duration={:?}",
                self.input_rate,
                self.output_rate,
                self.duration
            );

            let mut resampler = resampler.write().unwrap();

            Ok(resampler
                .resample(decoded)
                .ok_or(AudioOutputError::StreamEnd)?
                .to_vec())
        } else {
            log::debug!(
                "Passing through audio frames={} duration={duration} rate={} channels={} channels_count={}",
                decoded.frames(),
                spec.rate,
                spec.channels,
                spec.channels.count(),
            );
            Ok(to_samples(decoded))
        }
    }
}

impl Default for AacEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEncoder for AacEncoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("AacEncoder encode {} frames", decoded.frames());

        let decoded = self.resample_if_needed(decoded)?;

        Ok(self.encode_output(&decoded))
    }

    fn spec(&self) -> SignalSpec {
        SignalSpec {
            rate: self.output_rate as u32,
            channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        }
    }
}

impl AudioOutput for AacEncoder {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        if self.writer.is_none() {
            return Ok(0);
        }

        let bytes = self.encode(decoded)?;

        if let Some(writer) = self.writer.as_mut() {
            let mut count = 0;
            loop {
                count += match writer.write(&bytes[count..]) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        log::error!("Failed to write: {e:?}");
                        return Err(AudioOutputError::StreamClosed);
                    }
                };
                if count >= bytes.len() {
                    break;
                }
            }
        }

        Ok(bytes.len())
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        Ok(())
    }
}

pub fn encode_aac_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_aac_spawn(path, writer);

    stream
}

pub fn encode_aac_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    moosicbox_task::spawn_blocking("symphonia_player: encode_aac", move || {
        encode_aac(path, writer)
    })
}

pub fn encode_aac<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                AacEncoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to aac: {err:?}");
    }
}
