# Third-Party Licenses

Yogurt is MIT licensed (see [LICENSE](LICENSE)).
This file lists every crate compiled into the release binary and the
license each one ships under, generated from the actual dependency graph
in `Cargo.lock` rather than maintained by hand.

All 430 third-party dependencies below use a permissive license (MIT,
Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, Unlicense, BSL-1.0, or
CDLA-Permissive-2.0, several offered as a choice of two or more), all
compatible with distributing yogurt under MIT.
No copyleft license (GPL, AGPL) appears as a required option for any
dependency.

## Regenerating this file

```bash
cargo install cargo-bundle-licenses
cargo bundle-licenses --format yaml --output /tmp/licenses.yaml
```

The tool has no built-in Markdown table output and its `yaml`/`json` output
embeds the full license text per crate, which runs several megabytes.
This file is that output compacted to crate name, version, SPDX license
expression, and repository URL, which is enough to audit compatibility
without shipping megabytes of duplicated license text in the repo.

Generated 2026-08-28 with cargo-bundle-licenses 4.2.0 against the
workspace `Cargo.lock`.
Re-run after any dependency bump you want to re-audit.

## Dependencies

| Crate | Version | License | Repository |
|---|---|---|---|
| `aes` | 0.8.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/block-ciphers |
| `ahash` | 0.8.12 | MIT OR Apache-2.0 | https://github.com/tkaitchuck/ahash |
| `aho-corasick` | 1.1.4 | Unlicense OR MIT | https://github.com/BurntSushi/aho-corasick |
| `alsa` | 0.9.1 | Apache-2.0/MIT | https://github.com/diwic/alsa-rs |
| `alsa-sys` | 0.3.1 | MIT | https://github.com/diwic/alsa-sys |
| `ammonia` | 4.1.2 | MIT OR Apache-2.0 | https://github.com/rust-ammonia/ammonia |
| `android_system_properties` | 0.1.5 | MIT/Apache-2.0 | https://github.com/nical/android_system_properties |
| `anstream` | 1.0.0 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| `anstyle` | 1.0.14 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| `anstyle-parse` | 1.0.0 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| `anstyle-query` | 1.1.5 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| `anstyle-wincon` | 3.0.11 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| `anyhow` | 1.0.102 | MIT OR Apache-2.0 | https://github.com/dtolnay/anyhow |
| `apple-cf` | 0.9.3 | MIT OR Apache-2.0 | https://github.com/doom-fish/apple-cf-rs |
| `apple-metal` | 0.8.8 | MIT OR Apache-2.0 | https://github.com/doom-fish/apple-metal-rs |
| `apple-native-keyring-store` | 1.0.0 | MIT OR Apache-2.0 | https://github.com/open-source-cooperative/apple-native-keyring-store.git |
| `async-broadcast` | 0.7.2 | MIT OR Apache-2.0 | https://github.com/smol-rs/async-broadcast |
| `async-channel` | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-channel |
| `async-executor` | 1.14.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-executor |
| `async-io` | 2.6.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-io |
| `async-lock` | 3.4.2 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-lock |
| `async-process` | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-process |
| `async-recursion` | 1.1.1 | MIT OR Apache-2.0 | https://github.com/dcchut/async-recursion |
| `async-signal` | 0.2.14 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-signal |
| `async-task` | 4.7.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-task |
| `async-trait` | 0.1.89 | MIT OR Apache-2.0 | https://github.com/dtolnay/async-trait |
| `atomic-waker` | 1.1.2 | Apache-2.0 OR MIT | https://github.com/smol-rs/atomic-waker |
| `axum` | 0.8.9 | MIT | https://github.com/tokio-rs/axum |
| `axum-core` | 0.5.6 | MIT | https://github.com/tokio-rs/axum |
| `axum-macros` | 0.5.1 | MIT | https://github.com/tokio-rs/axum |
| `base64` | 0.22.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| `bitflags` | 1.3.2 | MIT/Apache-2.0 | https://github.com/bitflags/bitflags |
| `bitflags` | 2.13.0 | MIT OR Apache-2.0 | https://github.com/bitflags/bitflags |
| `block-buffer` | 0.10.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| `block-padding` | 0.3.3 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| `block2` | 0.6.2 | MIT | https://github.com/madsmtm/objc2 |
| `blocking` | 1.6.2 | Apache-2.0 OR MIT | https://github.com/smol-rs/blocking |
| `bumpalo` | 3.20.3 | MIT OR Apache-2.0 | https://github.com/fitzgen/bumpalo |
| `byteorder` | 1.5.0 | Unlicense OR MIT | https://github.com/BurntSushi/byteorder |
| `bytes` | 1.12.0 | MIT | https://github.com/tokio-rs/bytes |
| `cbc` | 0.1.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/block-modes |
| `cesu8` | 1.1.0 | Apache-2.0/MIT | https://github.com/emk/cesu8-rs |
| `cfg-if` | 1.0.4 | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| `chrono` | 0.4.45 | MIT OR Apache-2.0 | https://github.com/chronotope/chrono |
| `cipher` | 0.4.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| `clap` | 4.6.1 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| `clap_builder` | 4.6.0 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| `clap_derive` | 4.6.1 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| `clap_lex` | 1.1.0 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| `colorchoice` | 1.0.5 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| `combine` | 4.6.7 | MIT | https://github.com/Marwes/combine |
| `concurrent-queue` | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/concurrent-queue |
| `core-foundation` | 0.10.1 | MIT OR Apache-2.0 | https://github.com/servo/core-foundation-rs |
| `core-foundation-sys` | 0.8.7 | MIT OR Apache-2.0 | https://github.com/servo/core-foundation-rs |
| `coreaudio-rs` | 0.11.3 | MIT/Apache-2.0 | https://github.com/RustAudio/coreaudio-rs.git |
| `coreaudio-sys` | 0.2.18 | MIT | https://github.com/RustAudio/coreaudio-sys.git |
| `cpal` | 0.15.3 | Apache-2.0 | https://github.com/rustaudio/cpal |
| `cpufeatures` | 0.2.17 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| `crossbeam-queue` | 0.3.12 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| `crossbeam-utils` | 0.8.21 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| `crypto-common` | 0.1.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| `cssparser` | 0.35.0 | MPL-2.0 | https://github.com/servo/rust-cssparser |
| `cssparser-macros` | 0.6.1 | MPL-2.0 | https://github.com/servo/rust-cssparser |
| `dasp_sample` | 0.11.0 | MIT OR Apache-2.0 | https://github.com/rustaudio/sample.git |
| `data-encoding` | 2.11.0 | MIT | https://github.com/ia0/data-encoding |
| `deranged` | 0.5.8 | MIT OR Apache-2.0 | https://github.com/jhpratt/deranged |
| `digest` | 0.10.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| `directories` | 5.0.1 | MIT OR Apache-2.0 | https://github.com/soc/directories-rs |
| `dirs-sys` | 0.4.1 | MIT OR Apache-2.0 | https://github.com/dirs-dev/dirs-sys-rs |
| `dispatch2` | 0.3.1 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `displaydoc` | 0.2.6 | MIT OR Apache-2.0 | https://github.com/yaahc/displaydoc |
| `doom-fish-utils` | 0.3.3 | MIT OR Apache-2.0 | https://github.com/doom-fish/doom-fish-utils |
| `dotenvy` | 0.15.7 | MIT | https://github.com/allan2/dotenvy |
| `dtoa` | 1.0.11 | MIT OR Apache-2.0 | https://github.com/dtolnay/dtoa |
| `dtoa-short` | 0.3.5 | MPL-2.0 | https://github.com/upsuper/dtoa-short |
| `endi` | 1.1.1 | MIT | https://github.com/zeenix/endi |
| `enumflags2` | 0.7.12 | MIT OR Apache-2.0 | https://github.com/meithecatte/enumflags2 |
| `enumflags2_derive` | 0.7.12 | MIT OR Apache-2.0 | https://github.com/meithecatte/enumflags2 |
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/equivalent |
| `errno` | 0.3.14 | MIT OR Apache-2.0 | https://github.com/lambda-fairy/rust-errno |
| `event-listener` | 5.4.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/event-listener |
| `event-listener-strategy` | 0.5.4 | Apache-2.0 OR MIT | https://github.com/smol-rs/event-listener-strategy |
| `eventsource-stream` | 0.2.3 | MIT OR Apache-2.0 | https://github.com/jpopesculian/eventsource-stream |
| `fallible-iterator` | 0.3.0 | MIT/Apache-2.0 | https://github.com/sfackler/rust-fallible-iterator |
| `fallible-streaming-iterator` | 0.1.9 | MIT/Apache-2.0 | https://github.com/sfackler/fallible-streaming-iterator |
| `fastrand` | 2.4.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/fastrand |
| `fnv` | 1.0.7 | Apache-2.0 / MIT | https://github.com/servo/rust-fnv |
| `form_urlencoded` | 1.2.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| `futf` | 0.1.5 | MIT / Apache-2.0 | https://github.com/servo/futf |
| `futures-channel` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `futures-core` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `futures-io` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `futures-lite` | 2.6.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/futures-lite |
| `futures-macro` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `futures-sink` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `futures-task` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `futures-util` | 0.3.32 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| `generic-array` | 0.14.7 | MIT | https://github.com/fizyk20/generic-array.git |
| `getrandom` | 0.2.17 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| `getrandom` | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| `h2` | 0.4.15 | MIT | https://github.com/hyperium/h2 |
| `hashbrown` | 0.14.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| `hashlink` | 0.9.1 | MIT OR Apache-2.0 | https://github.com/kyren/hashlink |
| `heck` | 0.5.0 | MIT OR Apache-2.0 | https://github.com/withoutboats/heck |
| `hermit-abi` | 0.5.2 | MIT OR Apache-2.0 | https://github.com/hermit-os/hermit-rs |
| `hex` | 0.4.3 | MIT OR Apache-2.0 | https://github.com/KokaKiwi/rust-hex |
| `hkdf` | 0.12.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/KDFs/ |
| `hmac` | 0.12.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/MACs |
| `html-escape` | 0.2.13 | MIT | https://github.com/magiclen/html-escape |
| `html5ever` | 0.35.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| `http` | 1.4.2 | MIT OR Apache-2.0 | https://github.com/hyperium/http |
| `http-body` | 1.0.1 | MIT | https://github.com/hyperium/http-body |
| `http-body-util` | 0.1.3 | MIT | https://github.com/hyperium/http-body |
| `http-range-header` | 0.4.2 | MIT | https://github.com/MarcusGrass/parse-range-headers |
| `httparse` | 1.10.1 | MIT OR Apache-2.0 | https://github.com/seanmonstar/httparse |
| `httpdate` | 1.0.3 | MIT OR Apache-2.0 | https://github.com/pyfisch/httpdate |
| `hyper` | 1.10.1 | MIT | https://github.com/hyperium/hyper |
| `hyper-rustls` | 0.27.9 | Apache-2.0 OR ISC OR MIT | https://github.com/rustls/hyper-rustls |
| `hyper-util` | 0.1.20 | MIT | https://github.com/hyperium/hyper-util |
| `iana-time-zone` | 0.1.65 | MIT OR Apache-2.0 | https://github.com/strawlab/iana-time-zone |
| `iana-time-zone-haiku` | 0.1.2 | MIT OR Apache-2.0 | https://github.com/strawlab/iana-time-zone |
| `icu_collections` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `icu_locale_core` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `icu_normalizer` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `icu_normalizer_data` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `icu_properties` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `icu_properties_data` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `icu_provider` | 2.2.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `idna` | 1.1.0 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| `idna_adapter` | 1.2.2 | Apache-2.0 OR MIT | https://github.com/hsivonen/idna_adapter |
| `indexmap` | 2.14.0 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/indexmap |
| `inout` | 0.1.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| `ipnet` | 2.12.0 | MIT OR Apache-2.0 | https://github.com/krisprice/ipnet |
| `is-docker` | 0.2.0 | MIT | https://github.com/TheLarkInn/is-docker |
| `is-wsl` | 0.4.0 | MIT | https://github.com/TheLarkInn/is-wsl |
| `is_terminal_polyfill` | 1.70.2 | MIT OR Apache-2.0 | https://github.com/polyfill-rs/is_terminal_polyfill |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 | https://github.com/dtolnay/itoa |
| `jni` | 0.21.1 | MIT/Apache-2.0 | https://github.com/jni-rs/jni-rs |
| `jni-sys` | 0.3.1 | MIT OR Apache-2.0 | https://github.com/jni-rs/jni-sys |
| `jni-sys` | 0.4.1 | MIT OR Apache-2.0 | https://github.com/jni-rs/jni-sys |
| `jni-sys-macros` | 0.4.1 | MIT OR Apache-2.0 | https://github.com/jni-rs/jni-sys |
| `js-sys` | 0.3.103 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys |
| `keyring` | 4.1.2 | MIT OR Apache-2.0 | https://github.com/open-source-cooperative/keyring-rs |
| `keyring-core` | 1.0.0 | MIT OR Apache-2.0 | https://github.com/open-source-cooperative/keyring-core.git |
| `lazy_static` | 1.5.0 | MIT OR Apache-2.0 | https://github.com/rust-lang-nursery/lazy-static.rs |
| `libc` | 0.2.186 | MIT OR Apache-2.0 | https://github.com/rust-lang/libc |
| `libredox` | 0.1.17 | MIT | https://gitlab.redox-os.org/redox-os/libredox.git |
| `libsqlite3-sys` | 0.30.1 | MIT | https://github.com/rusqlite/rusqlite |
| `linux-raw-sys` | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/sunfishcode/linux-raw-sys |
| `litemap` | 0.8.2 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `lock_api` | 0.4.14 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| `log` | 0.4.33 | MIT OR Apache-2.0 | https://github.com/rust-lang/log |
| `lru-slab` | 0.1.2 | MIT OR Apache-2.0 OR Zlib | https://github.com/Ralith/lru-slab |
| `mac` | 0.1.1 | MIT/Apache-2.0 | https://github.com/reem/rust-mac.git |
| `mach2` | 0.4.3 | BSD-2-Clause OR MIT OR Apache-2.0 | https://github.com/JohnTitor/mach2 |
| `maplit` | 1.0.2 | MIT/Apache-2.0 | https://github.com/bluss/maplit |
| `markup5ever` | 0.35.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| `match_token` | 0.35.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| `matchers` | 0.2.0 | MIT | https://github.com/hawkw/matchers |
| `matchit` | 0.8.4 | MIT AND BSD-3-Clause | https://github.com/ibraheemdev/matchit |
| `memchr` | 2.8.2 | Unlicense OR MIT | https://github.com/BurntSushi/memchr |
| `memoffset` | 0.9.1 | MIT | https://github.com/Gilnaa/memoffset |
| `mime` | 0.3.17 | MIT OR Apache-2.0 | https://github.com/hyperium/mime |
| `mime_guess` | 2.0.5 | MIT | https://github.com/abonander/mime_guess |
| `minimal-lexical` | 0.2.1 | MIT/Apache-2.0 | https://github.com/Alexhuszagh/minimal-lexical |
| `mio` | 1.2.1 | MIT | https://github.com/tokio-rs/mio |
| `ndk` | 0.8.0 | MIT OR Apache-2.0 | https://github.com/rust-mobile/ndk |
| `ndk-context` | 0.1.1 | MIT OR Apache-2.0 | https://github.com/rust-windowing/android-ndk-rs |
| `ndk-sys` | 0.5.0+25.2.9519653 | MIT OR Apache-2.0 | https://github.com/rust-mobile/ndk |
| `new_debug_unreachable` | 1.0.6 | MIT | https://github.com/mbrubeck/rust-debug-unreachable |
| `nom` | 7.1.3 | MIT | https://github.com/Geal/nom |
| `nu-ansi-term` | 0.50.3 | MIT | https://github.com/nushell/nu-ansi-term |
| `num` | 0.4.3 | MIT OR Apache-2.0 | https://github.com/rust-num/num |
| `num-bigint` | 0.4.6 | MIT OR Apache-2.0 | https://github.com/rust-num/num-bigint |
| `num-complex` | 0.4.6 | MIT OR Apache-2.0 | https://github.com/rust-num/num-complex |
| `num-conv` | 0.2.2 | MIT OR Apache-2.0 | https://github.com/jhpratt/num-conv |
| `num-derive` | 0.4.2 | MIT OR Apache-2.0 | https://github.com/rust-num/num-derive |
| `num-integer` | 0.1.46 | MIT OR Apache-2.0 | https://github.com/rust-num/num-integer |
| `num-iter` | 0.1.45 | MIT OR Apache-2.0 | https://github.com/rust-num/num-iter |
| `num-rational` | 0.4.2 | MIT OR Apache-2.0 | https://github.com/rust-num/num-rational |
| `num-traits` | 0.2.19 | MIT OR Apache-2.0 | https://github.com/rust-num/num-traits |
| `num_enum` | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | https://github.com/illicitonion/num_enum |
| `num_enum_derive` | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | https://github.com/illicitonion/num_enum |
| `objc2` | 0.6.4 | MIT | https://github.com/madsmtm/objc2 |
| `objc2-av-foundation` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-avf-audio` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-audio` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-audio-types` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-foundation` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-graphics` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-image` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-media` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-core-video` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-encode` | 4.1.0 | MIT | https://github.com/madsmtm/objc2 |
| `objc2-foundation` | 0.3.2 | MIT | https://github.com/madsmtm/objc2 |
| `objc2-image-io` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-io-surface` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-media-toolbox` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `objc2-quartz-core` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| `oboe` | 0.6.1 | Apache-2.0 | https://github.com/katyo/oboe-rs |
| `oboe-sys` | 0.6.1 | Apache-2.0 | https://github.com/katyo/oboe-rs |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 | https://github.com/matklad/once_cell |
| `once_cell_polyfill` | 1.70.2 | MIT OR Apache-2.0 | https://github.com/polyfill-rs/once_cell_polyfill |
| `open` | 5.3.5 | MIT | https://github.com/Byron/open-rs |
| `option-ext` | 0.2.0 | MPL-2.0 | https://github.com/soc/option-ext.git |
| `ordered-stream` | 0.2.0 | MIT OR Apache-2.0 | https://github.com/danieldg/ordered-stream |
| `parking` | 2.2.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/parking |
| `parking_lot` | 0.12.5 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| `parking_lot_core` | 0.9.12 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| `pathdiff` | 0.2.3 | MIT/Apache-2.0 | https://github.com/Manishearth/pathdiff |
| `percent-encoding` | 2.3.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| `phf` | 0.11.3 | MIT | https://github.com/rust-phf/rust-phf |
| `phf_generator` | 0.11.3 | MIT | https://github.com/rust-phf/rust-phf |
| `phf_macros` | 0.11.3 | MIT | https://github.com/rust-phf/rust-phf |
| `phf_shared` | 0.11.3 | MIT | https://github.com/rust-phf/rust-phf |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project-lite |
| `piper` | 0.2.5 | MIT OR Apache-2.0 | https://github.com/smol-rs/piper |
| `polling` | 3.11.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/polling |
| `potential_utf` | 0.1.5 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `powerfmt` | 0.2.0 | MIT OR Apache-2.0 | https://github.com/jhpratt/powerfmt |
| `ppv-lite86` | 0.2.21 | MIT OR Apache-2.0 | https://github.com/cryptocorrosion/cryptocorrosion |
| `precomputed-hash` | 0.1.1 | MIT | https://github.com/emilio/precomputed-hash |
| `primal-check` | 0.3.4 | MIT OR Apache-2.0 | https://github.com/huonw/primal |
| `proc-macro-crate` | 3.5.0 | MIT OR Apache-2.0 | https://github.com/bkchr/proc-macro-crate |
| `proc-macro2` | 1.0.106 | MIT OR Apache-2.0 | https://github.com/dtolnay/proc-macro2 |
| `pulldown-cmark` | 0.12.2 | MIT | https://github.com/raphlinus/pulldown-cmark |
| `quinn` | 0.11.11 | MIT OR Apache-2.0 | https://github.com/quinn-rs/quinn |
| `quinn-proto` | 0.11.15 | MIT OR Apache-2.0 | https://github.com/quinn-rs/quinn |
| `quinn-udp` | 0.5.14 | MIT OR Apache-2.0 | https://github.com/quinn-rs/quinn |
| `quote` | 1.0.46 | MIT OR Apache-2.0 | https://github.com/dtolnay/quote |
| `r-efi` | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | https://github.com/r-efi/r-efi |
| `r-efi` | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | https://github.com/r-efi/r-efi |
| `rand` | 0.8.6 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| `rand` | 0.9.4 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| `rand_chacha` | 0.3.1 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| `rand_chacha` | 0.9.0 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| `rand_core` | 0.6.4 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| `rand_core` | 0.9.5 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| `realfft` | 3.5.0 | MIT | https://github.com/HEnquist/realfft |
| `redox_syscall` | 0.5.18 | MIT | https://gitlab.redox-os.org/redox-os/syscall |
| `redox_users` | 0.4.6 | MIT | https://gitlab.redox-os.org/redox-os/users |
| `regex` | 1.12.4 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| `regex-automata` | 0.4.14 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| `regex-syntax` | 0.8.11 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| `reqwest` | 0.12.28 | MIT OR Apache-2.0 | https://github.com/seanmonstar/reqwest |
| `ring` | 0.17.14 | Apache-2.0 AND ISC | https://github.com/briansmith/ring |
| `rtrb` | 0.3.4 | MIT OR Apache-2.0 | https://github.com/mgeier/rtrb |
| `rubato` | 0.16.2 | MIT | https://github.com/HEnquist/rubato |
| `rusqlite` | 0.32.1 | MIT | https://github.com/rusqlite/rusqlite |
| `rusqlite_migration` | 1.3.1 | Apache-2.0 | https://github.com/cljoly/rusqlite_migration |
| `rust-embed` | 8.11.0 | MIT | https://pyrossh.dev/repos/rust-embed |
| `rust-embed-impl` | 8.11.0 | MIT | https://pyrossh.dev/repos/rust-embed |
| `rust-embed-utils` | 8.11.0 | MIT | https://pyrossh.dev/repos/rust-embed |
| `rustc-hash` | 2.1.2 | Apache-2.0 OR MIT | https://github.com/rust-lang/rustc-hash |
| `rustfft` | 6.4.1 | MIT OR Apache-2.0 | https://github.com/ejmahler/RustFFT |
| `rustix` | 1.1.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/rustix |
| `rustls` | 0.23.41 | Apache-2.0 OR ISC OR MIT | https://github.com/rustls/rustls |
| `rustls-pki-types` | 1.14.1 | MIT OR Apache-2.0 | https://github.com/rustls/pki-types |
| `rustls-webpki` | 0.103.13 | ISC | https://github.com/rustls/webpki |
| `rustversion` | 1.0.22 | MIT OR Apache-2.0 | https://github.com/dtolnay/rustversion |
| `ryu` | 1.0.23 | Apache-2.0 OR BSL-1.0 | https://github.com/dtolnay/ryu |
| `same-file` | 1.0.6 | Unlicense/MIT | https://github.com/BurntSushi/same-file |
| `scopeguard` | 1.2.0 | MIT OR Apache-2.0 | https://github.com/bluss/scopeguard |
| `screencapturekit` | 8.0.0 | MIT OR Apache-2.0 | https://github.com/doom-fish/screencapturekit-rs |
| `secret-service` | 5.1.0 | MIT OR Apache-2.0 | https://github.com/hwchen/secret-service-rs.git |
| `security-framework` | 3.7.0 | MIT OR Apache-2.0 | https://github.com/kornelski/rust-security-framework |
| `security-framework-sys` | 2.17.0 | MIT OR Apache-2.0 | https://github.com/kornelski/rust-security-framework |
| `serde` | 1.0.228 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| `serde_core` | 1.0.228 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| `serde_derive` | 1.0.228 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| `serde_json` | 1.0.150 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| `serde_path_to_error` | 0.1.20 | MIT OR Apache-2.0 | https://github.com/dtolnay/path-to-error |
| `serde_repr` | 0.1.20 | MIT OR Apache-2.0 | https://github.com/dtolnay/serde-repr |
| `serde_urlencoded` | 0.7.1 | MIT/Apache-2.0 | https://github.com/nox/serde_urlencoded |
| `sha1` | 0.10.6 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| `sharded-slab` | 0.1.7 | MIT | https://github.com/hawkw/sharded-slab |
| `signal-hook-registry` | 1.4.8 | MIT OR Apache-2.0 | https://github.com/vorner/signal-hook |
| `siphasher` | 1.0.3 | MIT/Apache-2.0 | https://github.com/jedisct1/rust-siphash |
| `slab` | 0.4.12 | MIT | https://github.com/tokio-rs/slab |
| `smallvec` | 1.15.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-smallvec |
| `socket2` | 0.6.4 | MIT OR Apache-2.0 | https://github.com/rust-lang/socket2 |
| `stable_deref_trait` | 1.2.1 | MIT OR Apache-2.0 | https://github.com/storyyeller/stable_deref_trait |
| `strength_reduce` | 0.2.4 | MIT OR Apache-2.0 | http://github.com/ejmahler/strength_reduce |
| `string_cache` | 0.8.9 | MIT OR Apache-2.0 | https://github.com/servo/string-cache |
| `strsim` | 0.11.1 | MIT | https://github.com/rapidfuzz/strsim-rs |
| `subtle` | 2.6.1 | BSD-3-Clause | https://github.com/dalek-cryptography/subtle |
| `syn` | 2.0.118 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| `sync_wrapper` | 1.0.2 | Apache-2.0 | https://github.com/Actyx/sync_wrapper |
| `synstructure` | 0.13.2 | MIT | https://github.com/mystor/synstructure |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | https://github.com/Stebalien/tempfile |
| `tendril` | 0.4.3 | MIT/Apache-2.0 | https://github.com/servo/tendril |
| `thiserror` | 1.0.69 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| `thiserror` | 2.0.18 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| `thiserror-impl` | 1.0.69 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| `thiserror-impl` | 2.0.18 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| `thread_local` | 1.1.9 | MIT OR Apache-2.0 | https://github.com/Amanieu/thread_local-rs |
| `time` | 0.3.51 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| `time-core` | 0.1.9 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| `time-macros` | 0.2.30 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| `tinystr` | 0.8.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `tinytemplate` | 1.2.1 | Apache-2.0 OR MIT | https://github.com/bheisler/TinyTemplate |
| `tinyvec` | 1.11.0 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/tinyvec |
| `tinyvec_macros` | 0.1.1 | MIT OR Apache-2.0 OR Zlib | https://github.com/Soveu/tinyvec_macros |
| `tokio` | 1.52.3 | MIT | https://github.com/tokio-rs/tokio |
| `tokio-macros` | 2.7.0 | MIT | https://github.com/tokio-rs/tokio |
| `tokio-rustls` | 0.26.4 | MIT OR Apache-2.0 | https://github.com/rustls/tokio-rustls |
| `tokio-tungstenite` | 0.24.0 | MIT | https://github.com/snapview/tokio-tungstenite |
| `tokio-tungstenite` | 0.29.0 | MIT | https://github.com/snapview/tokio-tungstenite |
| `tokio-util` | 0.7.18 | MIT | https://github.com/tokio-rs/tokio |
| `toml_datetime` | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| `toml_edit` | 0.25.12+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| `toml_parser` | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| `tower` | 0.5.3 | MIT | https://github.com/tower-rs/tower |
| `tower-http` | 0.6.11 | MIT | https://github.com/tower-rs/tower-http |
| `tower-layer` | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| `tower-service` | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| `tracing` | 0.1.44 | MIT | https://github.com/tokio-rs/tracing |
| `tracing-attributes` | 0.1.31 | MIT | https://github.com/tokio-rs/tracing |
| `tracing-core` | 0.1.36 | MIT | https://github.com/tokio-rs/tracing |
| `tracing-log` | 0.2.0 | MIT | https://github.com/tokio-rs/tracing |
| `tracing-subscriber` | 0.3.23 | MIT | https://github.com/tokio-rs/tracing |
| `transpose` | 0.2.3 | MIT OR Apache-2.0 | https://github.com/ejmahler/transpose |
| `try-lock` | 0.2.5 | MIT | https://github.com/seanmonstar/try-lock |
| `tungstenite` | 0.24.0 | MIT OR Apache-2.0 | https://github.com/snapview/tungstenite-rs |
| `tungstenite` | 0.29.0 | MIT OR Apache-2.0 | https://github.com/snapview/tungstenite-rs |
| `typenum` | 1.20.1 | MIT OR Apache-2.0 | https://github.com/paholg/typenum |
| `uds_windows` | 1.2.1 | MIT | https://github.com/haraldh/rust_uds_windows |
| `ulid` | 1.2.1 | MIT | https://github.com/dylanhart/ulid-rs |
| `unicase` | 2.9.0 | MIT OR Apache-2.0 | https://github.com/seanmonstar/unicase |
| `unicode-ident` | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| `untrusted` | 0.9.0 | ISC | https://github.com/briansmith/untrusted |
| `url` | 2.5.8 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| `utf-8` | 0.7.6 | MIT OR Apache-2.0 | https://github.com/SimonSapin/rust-utf8 |
| `utf8-width` | 0.1.8 | MIT | https://github.com/magiclen/utf8-width |
| `utf8_iter` | 1.0.4 | Apache-2.0 OR MIT | https://github.com/hsivonen/utf8_iter |
| `utf8parse` | 0.2.2 | Apache-2.0 OR MIT | https://github.com/alacritty/vte |
| `uuid` | 1.23.4 | Apache-2.0 OR MIT | https://github.com/uuid-rs/uuid |
| `valuable` | 0.1.1 | MIT | https://github.com/tokio-rs/valuable |
| `walkdir` | 2.5.0 | Unlicense/MIT | https://github.com/BurntSushi/walkdir |
| `want` | 0.3.1 | MIT | https://github.com/seanmonstar/want |
| `wasi` | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wasi |
| `wasip2` | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wasi-rs |
| `wasm-bindgen` | 0.2.126 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen |
| `wasm-bindgen-futures` | 0.4.76 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures |
| `wasm-bindgen-macro` | 0.2.126 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro |
| `wasm-bindgen-macro-support` | 0.2.126 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro-support |
| `wasm-bindgen-shared` | 0.2.126 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared |
| `wasm-streams` | 0.4.2 | MIT OR Apache-2.0 | https://github.com/MattiasBuelens/wasm-streams/ |
| `web-sys` | 0.3.103 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys |
| `web-time` | 1.1.0 | MIT OR Apache-2.0 | https://github.com/daxpedda/web-time |
| `web_atoms` | 0.1.3 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| `webpki-roots` | 0.26.11 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| `webpki-roots` | 1.0.8 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| `webrtc-vad` | 0.4.0 | MIT | https://github.com/kaegi/webrtc-vad |
| `whisper-rs` | 0.16.0 | Unlicense | https://codeberg.org/tazz4843/whisper-rs |
| `whisper-rs-sys` | 0.15.0 | Unlicense | https://codeberg.org/tazz4843/whisper-rs |
| `winapi-util` | 0.1.11 | Unlicense OR MIT | https://github.com/BurntSushi/winapi-util |
| `windows` | 0.54.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-core` | 0.54.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-core` | 0.62.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-implement` | 0.60.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-interface` | 0.59.3 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-link` | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-native-keyring-store` | 1.1.0 | MIT OR Apache-2.0 | https://github.com/open-source-cooperative/windows-native-keyring-store.git |
| `windows-result` | 0.1.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-result` | 0.4.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-strings` | 0.5.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-sys` | 0.45.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-sys` | 0.48.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-sys` | 0.52.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-sys` | 0.60.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-targets` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-targets` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-targets` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows-targets` | 0.53.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_gnullvm` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_gnullvm` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_gnullvm` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_gnullvm` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_msvc` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_msvc` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_msvc` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_aarch64_msvc` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_gnu` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_gnu` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_gnu` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_gnu` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_gnullvm` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_gnullvm` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_msvc` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_msvc` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_msvc` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_i686_msvc` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnu` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnu` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnu` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnu` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnullvm` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnullvm` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnullvm` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_gnullvm` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_msvc` | 0.42.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_msvc` | 0.48.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_msvc` | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `windows_x86_64_msvc` | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| `winnow` | 1.0.3 | MIT | https://github.com/winnow-rs/winnow |
| `wit-bindgen` | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wit-bindgen |
| `writeable` | 0.6.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `yoke` | 0.8.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `yoke-derive` | 0.8.2 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `zbus` | 5.16.0 | MIT | https://github.com/z-galaxy/zbus/ |
| `zbus-secret-service-keyring-store` | 1.0.0 | MIT OR Apache-2.0 | https://github.com/open-source-cooperative/zbus-secret-service-keyring-store.git |
| `zbus_macros` | 5.16.0 | MIT | https://github.com/z-galaxy/zbus/ |
| `zbus_names` | 4.3.2 | MIT | https://github.com/z-galaxy/zbus/ |
| `zerocopy` | 0.8.52 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| `zerocopy-derive` | 0.8.52 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| `zerofrom` | 0.1.8 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `zerofrom-derive` | 0.1.7 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `zeroize` | 1.9.0 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| `zerotrie` | 0.2.4 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `zerovec` | 0.11.6 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `zerovec-derive` | 0.11.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| `zmij` | 1.0.21 | MIT | https://github.com/dtolnay/zmij |
| `zvariant` | 5.12.0 | MIT | https://github.com/z-galaxy/zbus/ |
| `zvariant_derive` | 5.12.0 | MIT | https://github.com/z-galaxy/zbus/ |
| `zvariant_utils` | 3.4.0 | MIT | https://github.com/z-galaxy/zbus/ |
