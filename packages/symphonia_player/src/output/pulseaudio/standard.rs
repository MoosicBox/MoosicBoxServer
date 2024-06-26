use moosicbox_env_utils::option_env_u32;
use pulse::def::BufferAttr;
use pulse::error::PAErr;
use pulse::time::MicroSeconds;
use symphonia::core::audio::SignalSpec;
use symphonia::core::units::Duration;
use thiserror::Error;

use std::ops::Deref;
use std::sync::atomic::AtomicUsize;
use std::{cell::RefCell, rc::Rc, time::SystemTime};

use crate::output::pulseaudio::common::map_channels_to_pa_channelmap;
use crate::output::{AudioOutput, AudioOutputError};
use crate::resampler::Resampler;

use pulse::context::{Context, FlagSet as ContextFlagSet};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::proplist::Proplist;
use pulse::stream::{FlagSet as StreamFlagSet, Latency, Stream};
use symphonia::core::audio::*;

use libpulse_binding as pulse;

pub struct PulseAudioOutput {
    mainloop: Rc<RefCell<Mainloop>>,
    stream: Rc<RefCell<pulse::stream::Stream>>,
    context: Rc<RefCell<pulse::context::Context>>,
    sample_buf: RawSampleBuffer<f32>,
    resampler: Option<Resampler<f32>>,
    bytes: AtomicUsize,
}

static SAMPLE_RATE: Option<u32> = option_env_u32!("PULSEAUDIO_RESAMPLE_RATE");

impl PulseAudioOutput {
    pub fn try_open(
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
        let pa = {
            // An interleaved buffer is required to send data to PulseAudio. Use a SampleBuffer to
            // move data between Symphonia AudioBuffers and the byte buffers required by PulseAudio.
            let sample_buf = RawSampleBuffer::<f32>::new(duration, spec);

            let pa_spec = {
                let spec = SignalSpec {
                    rate: SAMPLE_RATE.unwrap_or(spec.rate),
                    channels: spec.channels,
                };

                log::debug!(
                    "Creating PulseAudio stream with spec rate={} channels={}",
                    spec.rate,
                    spec.channels.count()
                );
                // Create a PulseAudio stream specification.
                pulse::sample::Spec {
                    format: pulse::sample::Format::FLOAT32NE,
                    channels: spec.channels.count() as u8,
                    rate: spec.rate,
                }
            };

            moosicbox_assert::assert!(pa_spec.is_valid());

            let pa_ch_map = map_channels_to_pa_channelmap(spec.channels);

            let mainloop = Rc::new(RefCell::new(
                Mainloop::new().expect("Failed to create mainloop"),
            ));

            let mut proplist = Proplist::new().unwrap();
            proplist
                .set_str(
                    pulse::proplist::properties::APPLICATION_NAME,
                    "MoosicBox Symphonia Player",
                )
                .unwrap();

            let context = Rc::new(RefCell::new(
                Context::new_with_proplist(mainloop.borrow().deref(), "FooAppContext", &proplist)
                    .expect("Failed to create new context"),
            ));

            {
                let mut ctx = context.borrow_mut();

                ctx.set_state_callback(Some(Box::new(|| log::trace!("Context STATE"))));
                ctx.set_event_callback(Some(Box::new(|evt, _props| {
                    log::trace!("Context EVENT: {evt}")
                })));
                ctx.set_subscribe_callback(Some(Box::new(|_facility, _operation, _index| {
                    log::trace!("Context SUBSCRIBED")
                })));

                ctx.connect(None, ContextFlagSet::NOFLAGS, None)
                    .expect("Failed to connect context");

                wait_for_context(
                    &mut mainloop.borrow_mut(),
                    &mut ctx,
                    pulse::context::State::Ready,
                )?;
            }

            let stream = Rc::new(RefCell::new(
                Stream::new(
                    &mut context.borrow_mut(),
                    "Music",
                    &pa_spec,
                    pa_ch_map.as_ref(),
                )
                .expect("Failed to create new stream"),
            ));

            {
                let mut strm = stream.borrow_mut();
                let buf_size = u32::pow(2, 15);
                let buf_attr = BufferAttr {
                    maxlength: buf_size * 4,
                    tlength: buf_size * 4,
                    prebuf: buf_size,
                    minreq: buf_size,
                    fragsize: buf_size,
                };
                strm.connect_playback(
                    None,
                    Some(&buf_attr),
                    StreamFlagSet::INTERPOLATE_TIMING
                        | StreamFlagSet::AUTO_TIMING_UPDATE
                        | StreamFlagSet::ADJUST_LATENCY
                        | StreamFlagSet::START_CORKED,
                    None,
                    None,
                )
                .expect("Failed to connect playback");

                strm.set_moved_callback(Some(Box::new(|| log::trace!("MOVED"))));
                strm.set_started_callback(Some(Box::new(|| log::trace!("STARTED"))));
                strm.set_overflow_callback(Some(Box::new(|| log::trace!("OVERFLOW"))));
                strm.set_underflow_callback(Some(Box::new(|| log::trace!("UNDERFLOW"))));
                strm.set_event_callback(Some(Box::new(|evt, _props| log::trace!("EVENT: {evt}"))));
                strm.set_suspended_callback(Some(Box::new(|| log::trace!("SUSPENDED"))));
                strm.set_latency_update_callback(Some(Box::new(|| log::trace!("LATENCY_UPDATE"))));
                strm.set_buffer_attr_callback(Some(Box::new(|| log::trace!("BUFFER_ATTR"))));
                strm.set_read_callback(Some(Box::new(|buf_size| log::trace!("READ: {buf_size}"))));
                strm.set_write_callback(Some(Box::new(move |buf_size| {
                    log::trace!("WRITE: {buf_size:?}");
                })));
            }

            let resampler = if let Some(rate) = SAMPLE_RATE {
                if spec.rate != rate {
                    log::info!("Resampling {} Hz to {} Hz", spec.rate, rate);
                    Some(Resampler::new(spec, rate as usize, duration))
                } else {
                    None
                }
            } else {
                None
            };

            PulseAudioOutput {
                mainloop: mainloop.clone(),
                stream: stream.clone(),
                context: context.clone(),
                sample_buf,
                resampler,
                bytes: AtomicUsize::new(0),
            }
        };

        Ok(Box::new(pa))
    }
}

#[derive(Debug, Error, Clone)]
enum MainloopError {
    #[error("Mainloop quit")]
    Quit,
    #[error("Mainloop error: {0:?}")]
    Error(PAErr),
}

impl<T> From<MainloopError> for StateError<T> {
    fn from(err: MainloopError) -> Self {
        StateError::Mainloop(err)
    }
}

#[derive(Debug, Error, Clone)]
enum StateError<T> {
    #[error(transparent)]
    Mainloop(MainloopError),
    #[error("Failure state: {0:?}")]
    State(T),
}

fn iterate_mainloop(mainloop: &mut Mainloop) -> Result<(), MainloopError> {
    match mainloop.iterate(false) {
        IterateResult::Quit(_) => Err(MainloopError::Quit),
        IterateResult::Err(error) => Err(MainloopError::Error(error)),
        IterateResult::Success(_) => Ok(()),
    }
}

fn wait_for_state<'a, T>(
    mainloop: &mut Mainloop,
    get_state: Box<dyn Fn() -> T + 'a>,
    expected_state: T,
    failure_states: &[T],
) -> Result<(), StateError<T>>
where
    T: std::fmt::Debug + PartialEq + Clone,
{
    let mut last_state = None;
    loop {
        iterate_mainloop(mainloop)?;
        let state = get_state();
        if state == expected_state {
            break Ok(());
        } else if !last_state.is_some_and(|s| s == state) {
            failure_states
                .iter()
                .find(|s| **s == state)
                .map(|s| Err::<(), _>(StateError::State((*s).clone())))
                .transpose()?;
            log::trace!("Stream state {state:?}");
        }
        last_state = Some(state);
    }
}

fn wait_for_context(
    mainloop: &mut Mainloop,
    context: &mut Context,
    expected_state: pulse::context::State,
) -> Result<(), AudioOutputError> {
    wait_for_state(
        mainloop,
        Box::new(|| context.get_state()),
        expected_state,
        &[
            pulse::context::State::Failed,
            pulse::context::State::Terminated,
        ],
    )
    .map_err(|e| match e {
        StateError::State(state) => {
            log::error!("Context failure state {:?}, quitting...", state);
            match state {
                pulse::context::State::Failed => AudioOutputError::StreamClosed,
                pulse::context::State::Terminated => AudioOutputError::StreamClosed,
                _ => unreachable!(),
            }
        }
        StateError::Mainloop(_) => AudioOutputError::StreamClosed,
    })
}

fn wait_for_stream(
    mainloop: &mut Mainloop,
    stream: &mut Stream,
    expected_state: pulse::stream::State,
) -> Result<(), AudioOutputError> {
    wait_for_state(
        mainloop,
        Box::new(|| stream.get_state()),
        expected_state,
        &[
            pulse::stream::State::Failed,
            pulse::stream::State::Terminated,
        ],
    )
    .map_err(|e| match e {
        StateError::State(state) => {
            log::error!("Stream failure state {:?}, quitting...", state);
            match state {
                pulse::stream::State::Failed => AudioOutputError::StreamClosed,
                pulse::stream::State::Terminated => AudioOutputError::StreamClosed,
                _ => unreachable!(),
            }
        }
        StateError::Mainloop(_) => AudioOutputError::StreamClosed,
    })
}

fn write_bytes(stream: &mut Stream, bytes: &[u8]) -> Result<usize, AudioOutputError> {
    let byte_count = bytes.len();
    let buffer = stream.begin_write(Some(byte_count)).unwrap().unwrap();
    buffer.copy_from_slice(bytes);

    let size_left = stream.writable_size().unwrap();
    // stream.begin_write(Some(byte_count)).unwrap();
    log::trace!("Writing to pulse audio {byte_count} bytes ({size_left} left)");
    // Write interleaved samples to PulseAudio.
    match stream.write(buffer, None, 0, pulse::stream::SeekMode::Relative) {
        Err(err) => {
            log::error!("audio output stream write error: {}", err);

            Err(AudioOutputError::StreamClosed)
        }
        _ => {
            if stream.is_corked().unwrap() {
                stream.uncork(None);
            }
            Ok(byte_count)
        }
    }
}

fn drain(mainloop: &mut Mainloop, stream: &mut Stream) -> Result<(), AudioOutputError> {
    log::trace!("Draining...");
    // Wait for our data to be played
    let drained = Rc::new(RefCell::new(false));
    let _o = {
        let drain_state_ref = Rc::clone(&drained);
        log::trace!("Attempting drain");
        stream.drain(Some(Box::new(move |success: bool| {
            log::trace!("Drain success: {success}");
            *drain_state_ref.borrow_mut() = true;
        })))
    };
    while !(*drained.borrow_mut()) {
        match mainloop.iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                log::error!("Iterate state was not success, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            IterateResult::Success(_) => {}
        }
    }
    *drained.borrow_mut() = false;
    log::trace!("Drained.");
    Ok(())
}

impl AudioOutput for PulseAudioOutput {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        let frame_count = decoded.frames();
        // Do nothing if there are no audio frames.
        if frame_count == 0 {
            log::trace!("No decoded frames. Returning");
            return Ok(0);
        }

        let bytes_vec = if let Some(resampler) = &mut self.resampler {
            let spec = resampler.spec;

            // Resampling is required. The resampler will return interleaved samples in the
            // correct sample format.
            let buf = match resampler.resample_to_audio_buffer(decoded) {
                Some(resampled) => resampled,
                None => return Ok(0),
            };

            let mut sample_buf = RawSampleBuffer::<f32>::new(buf.capacity() as u64, spec);
            sample_buf.copy_interleaved_typed(&buf);
            sample_buf.as_bytes().to_vec()
        } else {
            // Resampling is not required. Interleave the sample for cpal using a sample buffer.
            self.sample_buf.copy_interleaved_typed(&decoded);
            self.sample_buf.as_bytes().to_vec()
        };
        let mut bytes = bytes_vec.as_slice();

        // Wait for context to be ready
        wait_for_context(
            &mut self.mainloop.borrow_mut(),
            &mut self.context.borrow_mut(),
            pulse::context::State::Ready,
        )?;
        wait_for_stream(
            &mut self.mainloop.borrow_mut(),
            &mut self.stream.borrow_mut(),
            pulse::stream::State::Ready,
        )?;

        let mut bytes_available = self.stream.borrow().writable_size().unwrap();
        let latency = match self.stream.borrow().get_latency() {
            Ok(Latency::Positive(MicroSeconds(micros))) => {
                Some(std::time::Duration::from_micros(micros))
            }
            _ => None,
        };

        let mut bytes_written = 0;

        log::debug!("{bytes_available} bytes available");
        log::debug!("Latency {:?}", latency);

        let start = SystemTime::now();
        log::trace!("Writing bytes");

        while bytes_available < bytes.len() {
            if bytes_available > 0 {
                let write_now_bytes = &bytes[..bytes_available];
                bytes = &bytes[bytes_available..];

                log::trace!("Writing bytes (partial {bytes_available} bytes)");
                bytes_written += write_bytes(&mut self.stream.borrow_mut(), write_now_bytes)?;
            }

            iterate_mainloop(&mut self.mainloop.borrow_mut())
                .map_err(|_e| AudioOutputError::StreamClosed)?;

            bytes_available = self.stream.borrow().writable_size().unwrap();
        }

        bytes_written += write_bytes(&mut self.stream.borrow_mut(), bytes)?;

        let end = SystemTime::now();
        let took_ms = end.duration_since(start).unwrap().as_millis();

        let total_bytes = self
            .bytes
            .fetch_add(bytes_written, std::sync::atomic::Ordering::SeqCst)
            + bytes_written;

        log::trace!(
            "Successfully wrote to pulseaudio (total {total_bytes} bytes). Took {took_ms}ms"
        );

        if took_ms >= 500 {
            log::error!("Detected audio interrupt");
            return Err(AudioOutputError::Interrupt);
        }

        Ok(bytes_written)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        drain(
            &mut self.mainloop.borrow_mut(),
            &mut self.stream.borrow_mut(),
        )
    }
}

impl Drop for PulseAudioOutput {
    fn drop(&mut self) {
        log::debug!("Shutting PulseAudioOutput down");
        match self.stream.borrow_mut().disconnect() {
            Ok(()) => log::debug!("Disconnected stream"),
            Err(err) => log::error!("Failed to disconnect stream: {err:?}"),
        };
        match wait_for_stream(
            &mut self.mainloop.borrow_mut(),
            &mut self.stream.borrow_mut(),
            pulse::stream::State::Terminated,
        ) {
            Ok(()) => log::debug!("Terminated stream"),
            Err(err) => log::error!("Failed to terminate stream: {err:?}"),
        }

        self.context.borrow_mut().disconnect()
    }
}

pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    PulseAudioOutput::try_open(spec, duration)
}
