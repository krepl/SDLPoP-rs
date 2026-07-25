//! Streaming OGG/Vorbis decoder, pull-based, mirroring the small subset of
//! stb_vorbis's API that seg009.rs uses (open-from-memory, seek-to-start,
//! get-interleaved-i16-samples, close). Backed by symphonia (see docs/plans
//! Step B for the security/maintenance assessment behind this choice).
//!
//! stb_vorbis lets the caller ask for any channel count and silently
//! up/down-mixes; `get_samples_interleaved` below replicates that (mono <->
//! stereo duplication/averaging) since digi_audiospec always requests 2
//! channels regardless of the source file's channel count.

use std::io::Cursor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct OggDecoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    src_channels: usize,
    total_frames: u64,
    sample_buf: Option<SampleBuffer<i16>>,
    pending_pos: usize, // frame index already consumed from sample_buf
}

impl OggDecoder {
    pub fn open(data: Vec<u8>) -> Option<Box<OggDecoder>> {
        let mss = MediaSourceStream::new(
            Box::new(Cursor::new(data)),
            MediaSourceStreamOptions::default(),
        );
        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .ok()?;
        let reader = probed.format;
        let track = reader.default_track()?;
        let track_id = track.id;
        let src_channels = track.codec_params.channels?.count();
        let total_frames = track.codec_params.n_frames.unwrap_or(0);
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .ok()?;

        Some(Box::new(OggDecoder {
            reader,
            decoder,
            track_id,
            src_channels,
            total_frames,
            sample_buf: None,
            pending_pos: 0,
        }))
    }

    pub fn total_length_samples(&self) -> u64 {
        self.total_frames
    }

    pub fn seek_start(&mut self) {
        let _ = self.reader.seek(
            SeekMode::Accurate,
            SeekTo::TimeStamp { ts: 0, track_id: self.track_id },
        );
        self.decoder.reset();
        self.sample_buf = None;
        self.pending_pos = 0;
    }

    /// Fills `out` (interleaved, `out_channels` channels) with up to
    /// `out.len() / out_channels` frames. Returns the number of frames
    /// actually written (0 at end of stream).
    pub fn get_samples_interleaved(&mut self, out_channels: usize, out: &mut [i16]) -> usize {
        let frames_wanted = out.len() / out_channels;
        let mut frames_written = 0;
        while frames_written < frames_wanted {
            let avail = self.pending_frames();
            if avail == 0 {
                if !self.decode_next_packet() {
                    break;
                }
                continue;
            }
            let take = avail.min(frames_wanted - frames_written);
            let src_channels = self.src_channels;
            let samples = self.sample_buf.as_ref().unwrap().samples();
            for i in 0..take {
                let src_off = (self.pending_pos + i) * src_channels;
                let dst_off = (frames_written + i) * out_channels;
                Self::mix_frame(
                    &samples[src_off..src_off + src_channels],
                    &mut out[dst_off..dst_off + out_channels],
                );
            }
            self.pending_pos += take;
            frames_written += take;
        }
        frames_written
    }

    fn pending_frames(&self) -> usize {
        match &self.sample_buf {
            Some(buf) => buf.samples().len() / self.src_channels - self.pending_pos,
            None => 0,
        }
    }

    fn mix_frame(src: &[i16], dst: &mut [i16]) {
        let (src_ch, dst_ch) = (src.len(), dst.len());
        if src_ch == dst_ch {
            dst.copy_from_slice(src);
        } else if src_ch == 1 {
            dst.fill(src[0]);
        } else if dst_ch == 1 {
            let sum: i32 = src.iter().map(|&s| s as i32).sum();
            dst[0] = (sum / src_ch as i32) as i16;
        } else {
            for (i, d) in dst.iter_mut().enumerate() {
                *d = src[i % src_ch];
            }
        }
    }

    /// Decodes packets until one yields audio for our track, refilling
    /// `sample_buf`. Returns false on end-of-stream or unrecoverable error.
    fn decode_next_packet(&mut self) -> bool {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(_) => return false,
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            // Field-level (not method-level) borrows below, so the AudioBufferRef's borrow
            // of self.decoder can coexist with mutating self.sample_buf -- a self.fill_...()
            // helper would borrow all of self and conflict with the still-live audio_buf.
            let audio_buf = match self.decoder.decode(&packet) {
                Ok(buf) => buf,
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(_) => return false,
            };
            if self.sample_buf.is_none() {
                let spec = *audio_buf.spec();
                let capacity = audio_buf.capacity() as u64;
                self.sample_buf = Some(SampleBuffer::new(capacity, spec));
            }
            self.sample_buf.as_mut().unwrap().copy_interleaved_ref(audio_buf);
            self.pending_pos = 0;
            if self.pending_frames() > 0 {
                return true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OggDecoder;

    // digi_audiospec always requests 2 output channels regardless of the source file's
    // channel count, so get_samples_interleaved must up/down-mix like stb_vorbis did.
    #[test]
    fn mix_frame_matching_channels_copies_through() {
        let mut dst = [0i16; 2];
        OggDecoder::mix_frame(&[10, -20], &mut dst);
        assert_eq!(dst, [10, -20]);
    }

    #[test]
    fn mix_frame_mono_to_stereo_duplicates() {
        let mut dst = [0i16; 2];
        OggDecoder::mix_frame(&[42], &mut dst);
        assert_eq!(dst, [42, 42]);
    }

    #[test]
    fn mix_frame_stereo_to_mono_averages() {
        let mut dst = [0i16; 1];
        OggDecoder::mix_frame(&[10, 20], &mut dst);
        assert_eq!(dst, [15]);
    }
}
