//! Yogurt audio capture — 16 kHz mono i16 PCM, two channels (mic + system).
//!
//! Phase 2 scope: capture only. STT consumption is Phase 3.
//!
//! # Format contract
//!
//! Every [`Frame`] emitted by this crate is **16 kHz mono i16 PCM, 320 samples
//! (20 ms) per frame**. Phase 3 STT engines (Deepgram, whisper.cpp) consume
//! this format directly — never resample at the STT boundary. See the
//! crate-level [`README.md`](https://github.com/jarvisrchen/yogurt/blob/main/crates/yogurt-audio/README.md)
//! for the full contract.
//!
//! # Platform support
//!
//! - macOS 13+ (full surface): mic + system loopback.
//! - Other platforms: mic only via `cpal`; [`Channel::System`] returns
//!   [`AudioError::UnsupportedPlatform`].

#![deny(rust_2018_idioms, missing_debug_implementations)]

mod error;
mod frame;

pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
