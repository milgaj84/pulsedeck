//! Decryption for HLS: AES-128 segments and SAMPLE-AES audio.
//!
//! `METHOD=AES-128`: the whole segment is encrypted with AES-128 in CBC mode
//! and PKCS#7 padding.
//!
//! `METHOD=SAMPLE-AES`: only the audio samples are encrypted, in the same
//! cipher without padding, and the rest of the segment is in the clear.
//!
//! The cipher itself comes from the RustCrypto crates; this module only adds
//! the HLS rules around it (IV derivation, which bytes are protected, input
//! checks).

use super::playlist::HlsError;
use aes::Aes128;
use cbc::cipher::{
    block_padding::{NoPadding, Pkcs7},
    BlockModeDecrypt, KeyIvInit,
};

pub(crate) const KEY_LEN: usize = 16;
const BLOCK: usize = 16;

/// The IV for a segment: the playlist's explicit `IV` attribute, or the
/// segment's media sequence number as a big-endian 128-bit integer.
pub(crate) fn segment_iv(explicit: Option<[u8; 16]>, sequence: u64) -> [u8; 16] {
    explicit.unwrap_or_else(|| {
        let mut iv = [0u8; 16];
        iv[8..].copy_from_slice(&sequence.to_be_bytes());
        iv
    })
}

/// Decrypt one segment. Fails on a length that is not a whole number of
/// blocks, or on bad padding (which usually means the wrong key or IV).
pub(crate) fn decrypt_segment(
    data: &[u8],
    key: &[u8; KEY_LEN],
    iv: &[u8; 16],
) -> Result<Vec<u8>, HlsError> {
    if data.is_empty() || !data.len().is_multiple_of(BLOCK) {
        return Err(HlsError::Decrypt(
            "segment size is not a multiple of the AES block size",
        ));
    }
    cbc::Decryptor::<Aes128>::new(key.into(), iv.into())
        .decrypt_padded_vec::<Pkcs7>(data)
        .map_err(|_| HlsError::Decrypt("bad padding, so the key or IV is wrong"))
}

/// The key and IV a SAMPLE-AES segment is protected with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SampleAesKey {
    pub key: [u8; KEY_LEN],
    /// The playlist's IV, or the segment's sequence number (see [`segment_iv`]).
    /// Fragmented MP4 carries its own IVs and ignores this one.
    pub iv: [u8; 16],
}

/// Decrypt `region` (a whole number of blocks) in place with AES-128-CBC and no
/// padding. Returns the IV a following region continues with: the last
/// ciphertext block. `None` if `region` is not whole blocks or is empty.
pub(crate) fn cbc_decrypt_blocks(
    region: &mut [u8],
    key: &[u8; KEY_LEN],
    iv: &[u8; 16],
) -> Option<[u8; 16]> {
    if region.is_empty() || !region.len().is_multiple_of(BLOCK) {
        return None;
    }
    let last: [u8; 16] = region[region.len() - BLOCK..].try_into().ok()?;
    cbc::Decryptor::<Aes128>::new(key.into(), iv.into())
        .decrypt_padded::<NoPadding>(region)
        .ok()?;
    Some(last)
}

/// Decrypt the protected part of one SAMPLE-AES frame in place: after
/// `clear_prefix` bytes of header and 16 more in the clear come whole 16-byte
/// blocks of ciphertext, then up to 15 bytes in the clear. A frame too short to
/// hold a block after the clear part is not encrypted at all. Returns the last
/// ciphertext block (what a chained IV continues from), if any block was
/// decrypted.
pub(crate) fn decrypt_frame_with_iv(
    frame: &mut [u8],
    clear_prefix: usize,
    key: &[u8; KEY_LEN],
    iv: &[u8; 16],
) -> Option<[u8; 16]> {
    let protected = frame.get_mut(clear_prefix + BLOCK..)?;
    let whole = protected.len() / BLOCK * BLOCK;
    if whole == 0 {
        return None;
    }
    cbc_decrypt_blocks(&mut protected[..whole], key, iv)
}

/// Decrypt one SAMPLE-AES audio frame in place, with the IV starting over for
/// every frame. `clear_prefix` is the frame header (7 or 9 bytes of ADTS; none
/// for AC-3 and E-AC-3).
pub(crate) fn decrypt_sample_aes_frame(frame: &mut [u8], clear_prefix: usize, key: &SampleAesKey) {
    decrypt_frame_with_iv(frame, clear_prefix, &key.key, &key.iv);
}

/// The inverse of [`cbc_decrypt_blocks`], for building test data: encrypt
/// `region` (whole blocks) in place and return the last ciphertext block.
#[cfg(test)]
pub(crate) fn cbc_encrypt_blocks(
    region: &mut [u8],
    key: &[u8; KEY_LEN],
    iv: &[u8; 16],
) -> Option<[u8; 16]> {
    use cbc::cipher::BlockModeEncrypt;
    if region.is_empty() || !region.len().is_multiple_of(BLOCK) {
        return None;
    }
    let len = region.len();
    cbc::Encryptor::<Aes128>::new(key.into(), iv.into())
        .encrypt_padded::<NoPadding>(region, len)
        .ok()?;
    region[len - BLOCK..].try_into().ok()
}

/// The inverse of [`decrypt_sample_aes_frame`], for building test data.
#[cfg(test)]
pub(crate) fn encrypt_sample_aes_frame(frame: &mut [u8], clear_prefix: usize, key: &SampleAesKey) {
    let Some(protected) = frame.get_mut(clear_prefix + BLOCK..) else {
        return;
    };
    let whole = protected.len() / BLOCK * BLOCK;
    if whole > 0 {
        cbc_encrypt_blocks(&mut protected[..whole], &key.key, &key.iv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ];
    const PLAIN: &[u8] = b"hello hls aes-128 cbc";

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    // Vectors made with `openssl enc -aes-128-cbc -K 000102..0f -iv <iv>`.
    const CIPHER_EXPLICIT_IV: &str =
        "70be7e06a182ebb4de05bad164f12ea934b1ea24a0aefc6ae6960591b537fa87";
    const CIPHER_SEQUENCE_5: &str =
        "7c50cbf0562b185b98ee1c67384ffd815767647c6ead462334cd9791aaed2095";
    const CIPHER_EXACT_BLOCK: &str =
        "281567ab2f4cf0d73d3198225b8b83938e0d4fe286966ba47afeab038d2e3acc";

    #[test]
    fn decrypts_an_openssl_encrypted_segment_with_an_explicit_iv() {
        let out = decrypt_segment(&hex(CIPHER_EXPLICIT_IV), &KEY, &KEY).unwrap();
        assert_eq!(out, PLAIN);
    }

    #[test]
    fn decrypts_with_the_sequence_number_as_the_iv() {
        let iv = segment_iv(None, 5);

        let out = decrypt_segment(&hex(CIPHER_SEQUENCE_5), &KEY, &iv).unwrap();

        assert_eq!(out, PLAIN);
    }

    #[test]
    fn removes_a_whole_block_of_padding_after_an_exact_block() {
        let iv = segment_iv(None, 0);

        let out = decrypt_segment(&hex(CIPHER_EXACT_BLOCK), &KEY, &iv).unwrap();

        assert_eq!(out, b"0123456789abcdef");
    }

    #[test]
    fn the_sequence_iv_is_big_endian_in_the_low_eight_bytes() {
        assert_eq!(segment_iv(None, 0), [0; 16]);
        let iv = segment_iv(None, 0x0102_0304_0506_0708);
        assert_eq!(&iv[..8], &[0; 8]);
        assert_eq!(&iv[8..], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(segment_iv(None, u64::MAX)[8..], [0xFF; 8]);
    }

    #[test]
    fn an_explicit_iv_wins_over_the_sequence_number() {
        let explicit = [9u8; 16];
        assert_eq!(segment_iv(Some(explicit), 123), explicit);
    }

    #[test]
    fn a_wrong_iv_or_key_does_not_return_the_plaintext() {
        let wrong_iv = segment_iv(None, 6);
        // CBC only corrupts the first block with a wrong IV, so the padding
        // still checks out and the first 16 bytes are garbage.
        let out = decrypt_segment(&hex(CIPHER_SEQUENCE_5), &KEY, &wrong_iv).unwrap();
        assert_ne!(out, PLAIN);
        assert_eq!(out[16..], PLAIN[16..]);

        let mut wrong_key = KEY;
        wrong_key[0] ^= 1;
        let result = decrypt_segment(&hex(CIPHER_SEQUENCE_5), &wrong_key, &segment_iv(None, 5));
        assert!(result.map_or(true, |out| out != PLAIN));
    }

    #[test]
    fn rejects_empty_and_misaligned_input() {
        for len in [0, 1, 15, 17, 31] {
            let err = decrypt_segment(&vec![0u8; len], &KEY, &KEY).unwrap_err();
            assert!(matches!(err, HlsError::Decrypt(_)), "len {len}");
        }
    }

    #[test]
    fn bad_padding_is_an_error_not_a_panic() {
        let result = decrypt_segment(&[0u8; 32], &KEY, &KEY);

        assert!(matches!(result, Err(HlsError::Decrypt(_))));
    }

    mod property_tests {
        use super::*;
        use aes::cipher::BlockModeEncrypt;
        use proptest::prelude::*;

        fn encrypt(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Vec<u8> {
            cbc::Encryptor::<Aes128>::new(key.into(), iv.into()).encrypt_padded_vec::<Pkcs7>(data)
        }

        proptest! {
            #[test]
            fn encrypt_then_decrypt_round_trips(
                data in proptest::collection::vec(any::<u8>(), 0..2000),
                key in any::<[u8; 16]>(),
                sequence in any::<u64>(),
            ) {
                let iv = segment_iv(None, sequence);
                let encrypted = encrypt(&data, &key, &iv);

                prop_assert_eq!(encrypted.len() % 16, 0);
                prop_assert_eq!(decrypt_segment(&encrypted, &key, &iv).unwrap(), data);
            }

            #[test]
            fn arbitrary_input_never_panics(
                data in proptest::collection::vec(any::<u8>(), 0..300),
                key in any::<[u8; 16]>(),
                iv in any::<[u8; 16]>(),
            ) {
                let _ = decrypt_segment(&data, &key, &iv);
            }
        }
    }

    // --- SAMPLE-AES ----------------------------------------------------------

    const SAMPLE_IV: [u8; 16] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD,
        0xEF,
    ];
    const ADTS_HEADER: [u8; 7] = [0xFF, 0xF1, 0x50, 0x40, 0x0C, 0x9F, 0xFC];

    fn sample_key() -> SampleAesKey {
        SampleAesKey {
            key: KEY,
            iv: SAMPLE_IV,
        }
    }

    // Made with `openssl enc -aes-128-cbc -nopad -K 000102..0f -iv 0123..cdef`
    // from plaintext bytes 0x30.. (48 bytes) and 0x80.. (32 bytes).
    const PLAIN_48: &str = "303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f";
    const CIPHER_48: &str = "22fc24243ec806acf3f8ec043a6b3600495534fed8a811f68cb8411799a864d5c5fa73020ab69d0a3525e7086cdd2117";
    const PLAIN_32: &str = "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f";
    const CIPHER_32: &str = "102d6c9603f0d047e1883352be88fdcc3d32b68363a8fad0b4afa2e92a374193";

    /// header + 16 clear bytes + `protected` + `tail`.
    fn frame(header: &[u8], protected: &[u8], tail: &[u8]) -> Vec<u8> {
        let mut frame = header.to_vec();
        frame.extend_from_slice(&[0xEE; 16]);
        frame.extend_from_slice(protected);
        frame.extend_from_slice(tail);
        frame
    }

    #[test]
    fn decrypts_an_openssl_encrypted_sample_aes_frame() {
        let tail = [1, 2, 3, 4, 5];
        let mut encrypted = frame(&ADTS_HEADER, &hex(CIPHER_48), &tail);
        let expected = frame(&ADTS_HEADER, &hex(PLAIN_48), &tail);

        decrypt_sample_aes_frame(&mut encrypted, ADTS_HEADER.len(), &sample_key());

        assert_eq!(encrypted, expected);
    }

    #[test]
    fn the_iv_starts_over_for_every_frame() {
        // Two frames carrying the same ciphertext decrypt to the same plaintext:
        // the second does not continue the first one's chain.
        let key = sample_key();
        let mut first = frame(&ADTS_HEADER, &hex(CIPHER_32), &[]);
        let mut second = first.clone();

        decrypt_sample_aes_frame(&mut first, 7, &key);
        decrypt_sample_aes_frame(&mut second, 7, &key);

        assert_eq!(first, second);
        assert_eq!(&first[7 + 16..], hex(PLAIN_32));
    }

    #[test]
    fn a_nine_byte_header_moves_the_protected_part() {
        let header = [0xFF, 0xF0, 0x50, 0x40, 0x0C, 0x9F, 0xFC, 0xAA, 0xBB];
        let mut encrypted = frame(&header, &hex(CIPHER_32), &[]);

        decrypt_sample_aes_frame(&mut encrypted, header.len(), &sample_key());

        assert_eq!(encrypted, frame(&header, &hex(PLAIN_32), &[]));
    }

    #[test]
    fn frames_too_short_for_a_block_after_the_clear_part_are_untouched() {
        for protected in [0usize, 1, 15] {
            let original = frame(&ADTS_HEADER, &vec![0x5A; protected], &[]);
            let mut copy = original.clone();

            decrypt_sample_aes_frame(&mut copy, 7, &sample_key());

            assert_eq!(copy, original, "{protected} bytes after the clear part");
        }
        // Even shorter than the clear part itself.
        let mut tiny = ADTS_HEADER.to_vec();
        decrypt_sample_aes_frame(&mut tiny, 7, &sample_key());
        assert_eq!(tiny, ADTS_HEADER);
    }

    #[test]
    fn a_partial_last_block_stays_in_the_clear() {
        // 16 + 4 protected-region bytes: one block is decrypted, 4 bytes stay.
        let mut cipher = hex(CIPHER_32)[..16].to_vec();
        cipher.extend_from_slice(&[9, 9, 9, 9]);
        let mut encrypted = frame(&ADTS_HEADER, &cipher, &[]);

        decrypt_sample_aes_frame(&mut encrypted, 7, &sample_key());

        let mut expected_protected = hex(PLAIN_32)[..16].to_vec();
        expected_protected.extend_from_slice(&[9, 9, 9, 9]);
        assert_eq!(encrypted, frame(&ADTS_HEADER, &expected_protected, &[]));
    }

    #[test]
    fn cbc_blocks_report_the_chain_value_and_refuse_bad_lengths() {
        let cipher = hex(CIPHER_48);
        let mut region = cipher.clone();

        let next = cbc_decrypt_blocks(&mut region, &KEY, &SAMPLE_IV).unwrap();

        assert_eq!(region, hex(PLAIN_48));
        // The chain value is the last ciphertext block, which a following region continues from.
        assert_eq!(next.to_vec(), cipher[32..].to_vec());
        assert_eq!(cbc_decrypt_blocks(&mut [], &KEY, &SAMPLE_IV), None);
        assert_eq!(cbc_decrypt_blocks(&mut [0; 17], &KEY, &SAMPLE_IV), None);
    }

    #[test]
    fn the_encryptor_used_by_other_tests_agrees_with_openssl() {
        let mut region = hex(PLAIN_48);

        cbc_encrypt_blocks(&mut region, &KEY, &SAMPLE_IV).unwrap();

        assert_eq!(region, hex(CIPHER_48));
    }

    #[test]
    fn a_wrong_key_produces_garbage_not_a_panic() {
        let mut encrypted = frame(&ADTS_HEADER, &hex(CIPHER_48), &[]);
        let wrong = SampleAesKey {
            key: [0xFF; 16],
            iv: SAMPLE_IV,
        };

        decrypt_sample_aes_frame(&mut encrypted, 7, &wrong);

        assert_ne!(&encrypted[7 + 16..], hex(PLAIN_48));
    }

    proptest::proptest! {
        #[test]
        fn sample_aes_round_trips_for_any_frame(
            header_len in proptest::sample::select(vec![7usize, 9]),
            body in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..300),
            key in proptest::prelude::any::<[u8; 16]>(),
            iv in proptest::prelude::any::<[u8; 16]>(),
        ) {
            let key = SampleAesKey { key, iv };
            let mut original = vec![0xFF; header_len];
            original.extend_from_slice(&body);
            let mut frame = original.clone();

            encrypt_sample_aes_frame(&mut frame, header_len, &key);
            // The header and the 16 bytes after it are never touched.
            proptest::prop_assert_eq!(
                &frame[..(header_len + 16).min(frame.len())],
                &original[..(header_len + 16).min(original.len())]
            );
            decrypt_sample_aes_frame(&mut frame, header_len, &key);

            proptest::prop_assert_eq!(frame, original);
        }
    }
}
