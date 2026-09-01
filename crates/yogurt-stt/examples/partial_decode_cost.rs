//! Measures greedy ("partial preview") decode wall time against the amount
//! of audio handed to it, for one local model.
//!
//! This exists because the partial ticker in `whisper_local.rs` decodes the
//! whole in-flight utterance once a second, and "does that still fit inside
//! the 1 s tick" is a per-machine, per-model question that a comment cannot
//! answer on its own. AUD-1 chose round-robin over decoding both channels
//! every tick on the strength of these numbers; a future model with a fatter
//! decoder could change that answer, so leave a way to re-measure.
//!
//! Real speech matters here. A tone or white noise decodes to almost no
//! tokens, which hides the decoder-side cost entirely and makes the curve
//! look flat. Render some:
//!
//! ```sh
//! head -c 3000 scripts/eval/conversation.txt | tr '\n' ' ' > /tmp/speech.txt
//! say -f /tmp/speech.txt -o /tmp/speech.aiff
//! afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/speech.aiff /tmp/speech.wav
//! ```
//!
//! Then:
//!
//! ```sh
//! cargo run --release -p yogurt-stt --features local-stt \
//!     --example partial_decode_cost -- large-v3-turbo /tmp/speech.wav
//! ```
//!
//! Reading the output: the cost is `fixed encoder + per-token decoder`. The
//! fixed part dominates because whisper.cpp pads every input to a 30 s mel
//! window, so handing it 25 s of audio instead of 5 s costs far less than 5x.
//! What matters for the ticker is only that the `MAX_SEGMENT_MS` row stays
//! comfortably under 1000 ms.

use std::time::Instant;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Sample counts to probe, in seconds. 25 s is `vad::MAX_SEGMENT_MS`, the
/// largest buffer the ticker can ever be handed; 30 s is whisper's own
/// window, included to show where the padding stops helping.
const PROBE_SECONDS: [usize; 8] = [1, 3, 5, 10, 15, 20, 25, 30];

/// Repeats per probe. We report the best, not the mean — we are after the
/// cost of the work, not of whatever else the machine was doing.
const REPEATS: usize = 3;

fn main() {
    let mut args = std::env::args().skip(1);
    let model_id = args.next().unwrap_or_else(|| "large-v3-turbo".to_string());
    let wav_path = args
        .next()
        .expect("usage: partial_decode_cost <model-id> <16 kHz mono wav>");

    let pcm = read_wav_mono_16k(&wav_path);
    println!(
        "speech: {:.1} s from {wav_path}",
        pcm.len() as f32 / 16_000.0
    );

    let model = yogurt_stt::models::model_path(
        yogurt_stt::models::lookup(&model_id).expect("unknown model id"),
    )
    .expect("resolve model path");
    whisper_rs::install_logging_hooks();
    let ctx = WhisperContext::new_with_params(
        model.to_str().expect("model path is not utf-8"),
        WhisperContextParameters::default(),
    )
    .expect("load model");
    println!("model:  {model_id}\n");
    println!("{:>8}  {:>10}  {:>7}", "audio", "greedy", "words");

    for secs in PROBE_SECONDS {
        let want = 16_000 * secs;
        if pcm.len() < want {
            println!("{secs:>7}s  (wav too short, skipped)");
            continue;
        }
        let slice = &pcm[..want];
        let mut best = f64::MAX;
        let mut words = 0usize;
        for _ in 0..REPEATS {
            let t = Instant::now();
            // Mirror `WhisperLocal::decode(.., fast = true)` exactly — any
            // divergence here makes the numbers a measurement of something
            // the ticker does not actually run.
            let mut state = ctx.create_state().expect("create state");
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some("en"));
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_special(false);
            params.set_print_timestamps(false);
            params.set_no_context(true);
            state.full(params, slice).expect("decode");
            best = best.min(t.elapsed().as_secs_f64());
            words = (0..state.full_n_segments())
                .filter_map(|i| state.get_segment(i))
                .filter_map(|seg| seg.to_str_lossy().ok())
                .map(|text| text.split_whitespace().count())
                .sum();
        }
        let note = if secs == 25 {
            "  <- MAX_SEGMENT_MS"
        } else {
            ""
        };
        println!("{secs:>7}s  {:>7.0} ms  {words:>7}{note}", best * 1000.0);
    }
}

/// Minimal WAV reader: seek the `data` chunk, then reinterpret the rest as
/// little-endian i16. Deliberately not a dependency — this example only ever
/// reads files the header comment tells you how to produce, and `hound` is
/// not otherwise in the tree.
fn read_wav_mono_16k(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read wav");
    let data = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("no `data` chunk — is this a WAV?")
        + 8;
    bytes[data..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
        .collect()
}
