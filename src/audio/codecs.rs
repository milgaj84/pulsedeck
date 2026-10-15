//! The codec registry used for every stream.
//!
//! Rodio hard-codes Symphonia's built-in codec list, so PulseDeck drives
//! Symphonia itself and owns the registry. Third-party decoders are added here.

use std::sync::OnceLock;

use symphonia::core::codecs::CodecRegistry;

/// All codecs PulseDeck can decode, built once.
pub(super) fn codec_registry() -> &'static CodecRegistry {
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_all::<symphonia_adapter_libopus::OpusDecoder>();
        // Replaces Symphonia's AAC decoder, which cannot decode SBR (HE-AAC).
        registry.register_all::<super::aac_decoder::HeAacDecoder>();
        registry
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::codecs::{
        CODEC_TYPE_AAC, CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_OPUS, CODEC_TYPE_VORBIS,
    };

    #[test]
    fn registry_has_the_codecs_radio_streams_use() {
        let registry = codec_registry();
        for codec in [
            CODEC_TYPE_MP3,
            CODEC_TYPE_AAC,
            CODEC_TYPE_VORBIS,
            CODEC_TYPE_FLAC,
            CODEC_TYPE_OPUS,
        ] {
            assert!(registry.get_codec(codec).is_some(), "missing {codec:?}");
        }
    }
}
