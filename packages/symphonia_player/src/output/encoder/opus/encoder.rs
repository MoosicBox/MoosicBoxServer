use std::sync::{Mutex, RwLock};
use std::usize;

use crate::output::{AudioEncoder, AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::play_file_path_str;
use crate::resampler::Resampler;

use bytes::Bytes;
use lazy_static::lazy_static;
use moosicbox_converter::opus::{
    encoder_opus, OPUS_STREAM_COMMENTS_HEADER, OPUS_STREAM_IDENTIFICATION_HEADER,
};
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use ogg::{PacketWriteEndInfo, PacketWriter};
use symphonia::core::audio::*;
use symphonia::core::units::Duration;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

const STEREO_20MS: usize = 48000 * 2 * 20 / 1000;

pub struct OpusEncoder<'a> {
    buf: [f32; STEREO_20MS],
    buf_len: usize,
    packet_writer: PacketWriter<'a, Vec<u8>>,
    last_write_pos: usize,
    serial: u32,
    absgp: u64,
    time: usize,
    bytes_read: usize,
    resampler: Option<RwLock<Resampler<f32>>>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: Mutex<opus::Encoder>,
}

impl OpusEncoder<'_> {
    pub fn new() -> Self {
        let packet_writer = PacketWriter::new(Vec::new());

        Self {
            buf: [0.0; STEREO_20MS],
            buf_len: 0,
            packet_writer,
            last_write_pos: 0,
            serial: 0,
            absgp: 0,
            time: 0,
            bytes_read: 0,
            resampler: None,
            writer: None,
            encoder: Mutex::new(encoder_opus().unwrap()),
        }
    }

    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        let mut x = Self::new();
        x.writer.replace(Box::new(writer));
        x
    }

    pub fn open(&mut self, spec: SignalSpec, duration: Duration) {
        if spec.rate != 48000 {
            self.resampler
                .replace(RwLock::new(Resampler::new(spec, 48000_usize, duration)));
        } else {
            self.resampler.take();
        }
    }

    fn encode_output(&mut self, input: &[f32], buf_size: usize) -> Bytes {
        let mut read = 0;
        let mut written = vec![];
        let mut output_buf = vec![0_u8; buf_size];

        loop {
            log::trace!(
                "Encoding bytes to OPUS input_len={} buf_size={}",
                input.len(),
                buf_size
            );
            let info = moosicbox_converter::opus::encode_opus_float(
                &mut self.encoder.lock().unwrap(),
                &input[read..read + buf_size],
                &mut output_buf,
            )
            .expect("Failed to convert");

            log::trace!(
                "Encoded bytes to OPUS output_size={}/{buf_size} input_consumed={}",
                info.output_size,
                info.input_consumed
            );

            let len = info.output_size;
            let section = &output_buf[..info.output_size];

            if self.absgp == 0 {
                // https://datatracker.ietf.org/doc/html/rfc7845#section-5.1
                log::debug!("Writing OPUS identification header packet");
                self.packet_writer
                    .write_packet(
                        OPUS_STREAM_IDENTIFICATION_HEADER.to_vec(),
                        self.serial,
                        PacketWriteEndInfo::EndPage,
                        self.absgp,
                    )
                    .unwrap();

                // https://datatracker.ietf.org/doc/html/rfc7845#section-5.2
                log::debug!("Writing OPUS comments header packet");
                self.packet_writer
                    .write_packet(
                        OPUS_STREAM_COMMENTS_HEADER.to_vec(),
                        self.serial,
                        PacketWriteEndInfo::EndPage,
                        self.absgp,
                    )
                    .unwrap();
            }

            log::trace!("Writing OPUS packet of size {}", section.len());
            self.packet_writer
                .write_packet(
                    section.to_vec(),
                    self.serial,
                    PacketWriteEndInfo::NormalPacket,
                    self.absgp,
                )
                .expect("Failed to write packet");

            self.absgp += (info.input_consumed / 2) as u64;

            written.extend_from_slice(&self.write_new_packet_writer_contents());

            read += buf_size;
            if self.time % 1000 == 0 {
                log::debug!(
                    "Info: read={} written len={} input_consumed={} output_size={} len={}",
                    read,
                    written.len(),
                    buf_size,
                    len,
                    self.bytes_read
                );
            }

            if read >= input.len() {
                break;
            }
        }
        written.into()
    }

    fn write_new_packet_writer_contents(&mut self) -> Bytes {
        let writer_contents = self.packet_writer.inner();

        log::debug!(
            "last_write_pos={} current packet_writer len={}",
            self.last_write_pos,
            writer_contents.len()
        );
        if writer_contents.len() > self.last_write_pos {
            let written_section = &writer_contents[self.last_write_pos..];
            let written_section = written_section.to_vec();
            self.last_write_pos = writer_contents.len();

            log::trace!("OPUS packet writer data len={}", writer_contents.len());

            Bytes::from(written_section)
        } else {
            Bytes::new()
        }
    }

    fn write_samples(&mut self, decoded: Vec<f32>) -> Bytes {
        let samples = [self.buf[..self.buf_len].to_vec(), decoded].concat();

        self.buf_len = 0;

        let mut written = vec![];

        for chunk in samples.chunks(STEREO_20MS) {
            if chunk.len() < STEREO_20MS {
                self.buf_len = chunk.len();
                self.buf[..self.buf_len].copy_from_slice(chunk);
            } else {
                self.time += 20;
                log::debug!("Encoding OPUS chunk...");
                let bytes = self.encode_output(chunk, STEREO_20MS);
                let byte_count = bytes.len();
                log::debug!("Encoded OPUS chunk to {byte_count} bytes");
                written.extend_from_slice(&bytes);
                self.bytes_read += byte_count;
                if self.time % 1000 == 0 {
                    log::debug!("time: {}", self.time / 1000);
                }
            }
        }

        log::debug!("Encoded OPUS chunks to a total of {} bytes", written.len());

        written.into()
    }
}

fn to_samples(decoded: AudioBuffer<f32>) -> Vec<f32> {
    let n_channels = decoded.spec().channels.count();
    let n_samples = decoded.frames() * n_channels;
    let mut buf = vec![0_f32; n_samples];

    // Interleave the source buffer channels into the sample buffer.
    for ch in 0..n_channels {
        let ch_slice = decoded.chan(ch);

        for (dst, decoded) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
            *dst = *decoded;
        }
    }

    buf
}

impl AudioEncoder for OpusEncoder<'_> {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("OpusEncoder encode {} frames", decoded.frames());
        let buf = {
            if let Some(ref resampler) = self.resampler {
                log::debug!("Resampling");
                let mut buf = decoded.make_equivalent();
                decoded.convert(&mut buf);

                resampler
                    .write()
                    .unwrap()
                    .resample(buf)
                    .ok_or(AudioOutputError::StreamEnd)?
                    .to_vec()
            } else {
                log::debug!("Not resampling");
                to_samples(decoded)
            }
        };

        Ok(self.write_samples(buf))
    }
}

impl AudioOutput for OpusEncoder<'_> {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        if self.writer.is_none() {
            return Ok(0);
        }

        let bytes = self.encode(decoded)?;

        if let Some(writer) = self.writer.as_mut() {
            let mut count = 0;
            loop {
                count += writer.write(&bytes[count..]).unwrap();
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

pub fn encode_opus_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_opus_spawn(path, writer);

    stream
}

pub fn encode_opus_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move { encode_opus(path, writer) })
}

pub fn encode_opus<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            let mut encoder: OpusEncoder<'_> = OpusEncoder::with_writer(writer.clone());
            encoder.open(spec, duration);
            Ok(Box::new(encoder))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to opus: {err:?}");
    }
}
