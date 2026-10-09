//! AES-128 segment decryption for HLS (`#EXT-X-KEY:METHOD=AES-128`).
//!
//! The whole segment is encrypted with AES-128 in CBC mode and PKCS#7
//! padding. The cipher itself comes from the RustCrypto crates; this module
//! only adds the HLS rules around it (IV derivation, input checks).

use super::playlist::HlsError;
use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};

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
}
