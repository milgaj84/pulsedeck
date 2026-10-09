//! A small ID3v2 reader for HLS timed metadata. Pure: bytes in, text out.
//!
//! HLS carries "now playing" information as ID3 tags: in a dedicated MPEG-TS
//! stream (type 0x15) or in front of packed-audio segments. Only the artist
//! (`TPE1`) and title (`TIT2`) text frames are read; everything else (PRIV
//! timestamps, artwork URLs, ...) is skipped.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Id3Text {
    pub title: Option<String>,
    pub artist: Option<String>,
}

impl Id3Text {
    /// "Artist - Title", the form ICY streams use, or whichever half exists.
    pub(crate) fn display(&self) -> Option<String> {
        match (&self.artist, &self.title) {
            (Some(artist), Some(title)) => Some(format!("{artist} - {title}")),
            (None, Some(title)) => Some(title.clone()),
            (Some(artist), None) => Some(artist.clone()),
            (None, None) => None,
        }
    }

    fn merge(&mut self, other: Id3Text) {
        self.title = other.title.or(self.title.take());
        self.artist = other.artist.or(self.artist.take());
    }
}

fn syncsafe(bytes: &[u8]) -> Option<usize> {
    let bytes: &[u8; 4] = bytes.try_into().ok()?;
    if bytes.iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    Some(
        bytes
            .iter()
            .fold(0usize, |acc, b| (acc << 7) | usize::from(*b)),
    )
}

fn be32(bytes: &[u8]) -> Option<usize> {
    let bytes: &[u8; 4] = bytes.try_into().ok()?;
    Some(u32::from_be_bytes(*bytes) as usize)
}

/// Read every ID3v2 tag at the start of `buf` and merge their text frames.
pub(crate) fn parse_leading_tags(mut buf: &[u8]) -> Id3Text {
    let mut result = Id3Text::default();
    while buf.len() >= 10 && &buf[..3] == b"ID3" {
        let Some(size) = syncsafe(&buf[6..10]) else {
            break;
        };
        let footer = if buf[5] & 0x10 != 0 { 10 } else { 0 };
        let total = 10 + size + footer;
        let Some(body) = buf.get(10..10 + size) else {
            break;
        };
        result.merge(parse_tag_body(buf[3], buf[5], body));
        let Some(rest) = buf.get(total..) else {
            break;
        };
        buf = rest;
    }
    result
}

fn remove_unsynchronisation(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut iter = body.iter().peekable();
    while let Some(&byte) = iter.next() {
        out.push(byte);
        if byte == 0xFF && iter.peek() == Some(&&0x00) {
            iter.next();
        }
    }
    out
}

fn parse_tag_body(version: u8, flags: u8, body: &[u8]) -> Id3Text {
    if version != 3 && version != 4 {
        return Id3Text::default();
    }
    // v2.3 stuffs the whole tag (frame sizes count the unstuffed bytes), while
    // v2.4 stuffs each frame (frame sizes count the stuffed bytes).
    let unsynchronised;
    let mut body = if version == 3 && flags & 0x80 != 0 {
        unsynchronised = remove_unsynchronisation(body);
        &unsynchronised[..]
    } else {
        body
    };
    let tag_unsynchronised = version == 4 && flags & 0x80 != 0;

    if flags & 0x40 != 0 {
        // Extended header: syncsafe size including itself (v2.4), or a plain
        // size excluding the size field (v2.3).
        let skip = match version {
            4 => syncsafe(body.get(..4).unwrap_or_default()),
            _ => be32(body.get(..4).unwrap_or_default()).map(|size| size + 4),
        };
        match skip.and_then(|skip| body.get(skip..)) {
            Some(rest) => body = rest,
            None => return Id3Text::default(),
        }
    }

    let mut result = Id3Text::default();
    let mut pos = 0;
    while pos + 10 <= body.len() {
        let id = &body[pos..pos + 4];
        if id.iter().all(|b| *b == 0) {
            break;
        }
        let size = match version {
            4 => syncsafe(&body[pos + 4..pos + 8]),
            _ => be32(&body[pos + 4..pos + 8]),
        };
        let Some(size) = size else { break };
        let format_flags = body[pos + 9];
        let Some(content) = body.get(pos + 10..pos + 10 + size) else {
            break;
        };
        pos += 10 + size;

        // v2.4 format flags: bit 0 data-length indicator, bit 1 unsynchronised,
        // bit 2 encryption, bit 3 compression.
        let stuffed;
        let (compressed_or_encrypted, content) = if version == 4 {
            let skip_length = if format_flags & 0x01 != 0 { 4 } else { 0 };
            let content = content.get(skip_length..).unwrap_or_default();
            let content = if tag_unsynchronised || format_flags & 0x02 != 0 {
                stuffed = remove_unsynchronisation(content);
                &stuffed[..]
            } else {
                content
            };
            (format_flags & 0x0C != 0, content)
        } else {
            // v2.3 format flags: bit 7 compression, bit 6 encryption.
            (format_flags & 0xC0 != 0, content)
        };
        if compressed_or_encrypted {
            continue;
        }

        match id {
            b"TIT2" => result.title = decode_text_frame(content),
            b"TPE1" => result.artist = decode_text_frame(content),
            _ => {}
        }
    }
    result
}

/// Decode the first string of a text frame (encoding byte + text).
fn decode_text_frame(content: &[u8]) -> Option<String> {
    let (&encoding, text) = content.split_first()?;
    let decoded = match encoding {
        0 => text.iter().map(|b| char::from(*b)).collect::<String>(),
        1 => decode_utf16_with_bom(text),
        2 => decode_utf16(text, false),
        3 => String::from_utf8_lossy(text).into_owned(),
        _ => return None,
    };
    // Frames may hold several NUL-separated strings; keep the first.
    let first = decoded.split('\0').next().unwrap_or_default().trim();
    (!first.is_empty()).then(|| first.to_string())
}

fn decode_utf16_with_bom(text: &[u8]) -> String {
    match text {
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, false),
        // No BOM: the spec requires one, but assume little endian.
        rest => decode_utf16(rest, true),
    }
}

fn decode_utf16(text: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = text
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syncsafe_bytes(value: usize) -> [u8; 4] {
        [
            ((value >> 21) & 0x7F) as u8,
            ((value >> 14) & 0x7F) as u8,
            ((value >> 7) & 0x7F) as u8,
            (value & 0x7F) as u8,
        ]
    }

    /// A text frame: id + size + flags + encoding byte + text bytes.
    fn frame(version: u8, id: &str, encoding: u8, text: &[u8]) -> Vec<u8> {
        let size = 1 + text.len();
        let mut out = id.as_bytes().to_vec();
        if version == 4 {
            out.extend_from_slice(&syncsafe_bytes(size));
        } else {
            out.extend_from_slice(&(size as u32).to_be_bytes());
        }
        out.extend_from_slice(&[0, 0]);
        out.push(encoding);
        out.extend_from_slice(text);
        out
    }

    fn tag(version: u8, flags: u8, frames: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = frames.concat();
        let mut out = vec![b'I', b'D', b'3', version, 0, flags];
        out.extend_from_slice(&syncsafe_bytes(body.len()));
        out.extend(body);
        out
    }

    fn utf16_le(text: &str) -> Vec<u8> {
        let mut out = vec![0xFF, 0xFE];
        out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        out
    }

    #[test]
    fn reads_the_title_and_artist_from_a_utf8_v24_tag_like_rtl_sends() {
        let data = tag(
            4,
            0,
            &[
                frame(4, "TIT2", 3, "La tournée d'RTL à Nancy\0".as_bytes()),
                frame(4, "TPE1", 3, "Jade & Éric Silvestro\0".as_bytes()),
                frame(4, "TRSN", 3, b"RTL\0"),
            ],
        );

        let text = parse_leading_tags(&data);

        assert_eq!(text.title.as_deref(), Some("La tournée d'RTL à Nancy"));
        assert_eq!(text.artist.as_deref(), Some("Jade & Éric Silvestro"));
        assert_eq!(
            text.display().as_deref(),
            Some("Jade & Éric Silvestro - La tournée d'RTL à Nancy")
        );
    }

    #[test]
    fn an_empty_tag_like_france_inter_sends_yields_nothing() {
        let text = parse_leading_tags(&tag(4, 0, &[]));

        assert_eq!(text, Id3Text::default());
        assert_eq!(text.display(), None);
    }

    #[test]
    fn reads_v23_tags_with_plain_frame_sizes() {
        let data = tag(3, 0, &[frame(3, "TIT2", 0, b"Caf\xE9 del Mar")]);

        assert_eq!(
            parse_leading_tags(&data).title.as_deref(),
            Some("Café del Mar")
        );
    }

    #[test]
    fn decodes_every_text_encoding() {
        let cases: [(u8, Vec<u8>); 4] = [
            (0, b"Caf\xE9".to_vec()),
            (1, utf16_le("Café")),
            (
                2,
                "Café".encode_utf16().flat_map(u16::to_be_bytes).collect(),
            ),
            (3, "Café".as_bytes().to_vec()),
        ];
        for (encoding, text) in cases {
            let data = tag(4, 0, &[frame(4, "TIT2", encoding, &text)]);
            assert_eq!(
                parse_leading_tags(&data).title.as_deref(),
                Some("Café"),
                "encoding {encoding}"
            );
        }
    }

    #[test]
    fn utf16_big_endian_bom_and_missing_bom_are_handled() {
        let mut be = vec![0xFE, 0xFF];
        be.extend("Né".encode_utf16().flat_map(u16::to_be_bytes));
        let data = tag(4, 0, &[frame(4, "TIT2", 1, &be)]);
        assert_eq!(parse_leading_tags(&data).title.as_deref(), Some("Né"));

        let no_bom: Vec<u8> = "ok".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let data = tag(4, 0, &[frame(4, "TIT2", 1, &no_bom)]);
        assert_eq!(parse_leading_tags(&data).title.as_deref(), Some("ok"));
    }

    #[test]
    fn keeps_only_the_first_of_several_nul_separated_strings() {
        let data = tag(4, 0, &[frame(4, "TPE1", 3, b"First\0Second\0Third")]);

        assert_eq!(parse_leading_tags(&data).artist.as_deref(), Some("First"));
    }

    #[test]
    fn blank_and_unknown_encoding_frames_are_ignored() {
        let data = tag(
            4,
            0,
            &[
                frame(4, "TIT2", 3, b"   \0"),
                frame(4, "TPE1", 9, b"Artist"),
            ],
        );

        assert_eq!(parse_leading_tags(&data), Id3Text::default());
    }

    #[test]
    fn skips_frames_it_does_not_read() {
        let mut priv_frame = b"PRIV".to_vec();
        priv_frame.extend_from_slice(&syncsafe_bytes(30));
        priv_frame.extend_from_slice(&[0, 0]);
        priv_frame.extend(std::iter::repeat_n(0xFFu8, 30));
        let data = tag(4, 0, &[priv_frame, frame(4, "TIT2", 3, b"After")]);

        assert_eq!(parse_leading_tags(&data).title.as_deref(), Some("After"));
    }

    #[test]
    fn merges_consecutive_tags_and_lets_later_frames_win() {
        let mut data = tag(
            4,
            0,
            &[frame(4, "TIT2", 3, b"Old"), frame(4, "TPE1", 3, b"Artist")],
        );
        data.extend(tag(4, 0, &[frame(4, "TIT2", 3, b"New")]));
        data.extend_from_slice(b"AUDIO");

        let text = parse_leading_tags(&data);

        assert_eq!(text.display().as_deref(), Some("Artist - New"));
    }

    #[test]
    fn handles_the_extended_header_and_the_footer() {
        // v2.4 extended header: syncsafe size 6, 1 flag byte count, 1 flag byte.
        let mut ext = syncsafe_bytes(6).to_vec();
        ext.extend_from_slice(&[1, 0]);
        let body = [ext, frame(4, "TIT2", 3, b"Ext")].concat();
        let mut data = vec![b'I', b'D', b'3', 4, 0, 0x40];
        data.extend_from_slice(&syncsafe_bytes(body.len()));
        data.extend(body);
        assert_eq!(parse_leading_tags(&data).title.as_deref(), Some("Ext"));

        // v2.3 extended header: plain size 6 excluding the size field.
        let mut ext = 6u32.to_be_bytes().to_vec();
        ext.extend_from_slice(&[0; 6]);
        let body = [ext, frame(3, "TIT2", 3, b"Ext3")].concat();
        let mut data = vec![b'I', b'D', b'3', 3, 0, 0x40];
        data.extend_from_slice(&syncsafe_bytes(body.len()));
        data.extend(body);
        assert_eq!(parse_leading_tags(&data).title.as_deref(), Some("Ext3"));

        let mut footer = tag(4, 0x10, &[frame(4, "TIT2", 3, b"Foot")]);
        footer.extend_from_slice(&[0; 10]);
        assert_eq!(parse_leading_tags(&footer).title.as_deref(), Some("Foot"));
    }

    #[test]
    fn v23_unsynchronisation_applies_to_the_whole_tag() {
        // Latin-1 text "a", 0xFF, 0xE9. The frame size counts the unstuffed
        // bytes (4); the wire has a 0x00 stuffed after the 0xFF.
        let mut frame = b"TIT2".to_vec();
        frame.extend_from_slice(&4u32.to_be_bytes());
        frame.extend_from_slice(&[0, 0, 0, b'a', 0xFF, 0x00, 0xE9]);
        let mut data = vec![b'I', b'D', b'3', 3, 0, 0x80];
        data.extend_from_slice(&syncsafe_bytes(frame.len()));
        data.extend(frame);

        assert_eq!(
            parse_leading_tags(&data).title.as_deref(),
            Some("a\u{ff}\u{e9}")
        );
    }

    #[test]
    fn v24_unsynchronisation_applies_per_frame_and_sizes_count_stuffing() {
        let mut stuffed = frame(4, "TIT2", 0, &[b'a', 0xFF, 0x00, 0xE9]);
        stuffed[9] = 0x02; // frame is unsynchronised
        let plain = frame(4, "TPE1", 3, b"Artist");

        let data = tag(4, 0, &[stuffed.clone(), plain.clone()]);
        let text = parse_leading_tags(&data);
        assert_eq!(text.title.as_deref(), Some("a\u{ff}\u{e9}"));
        assert_eq!(text.artist.as_deref(), Some("Artist"));

        // The tag-level flag means every frame is unsynchronised.
        let data = tag(4, 0x80, &[stuffed.clone(), plain]);
        assert_eq!(
            parse_leading_tags(&data).title.as_deref(),
            Some("a\u{ff}\u{e9}")
        );

        // Without either flag the 0x00 is real data: it ends the string.
        stuffed[9] = 0;
        let data = tag(4, 0, &[stuffed]);
        assert_eq!(parse_leading_tags(&data).title.as_deref(), Some("a\u{ff}"));
    }

    #[test]
    fn truncated_or_corrupt_input_never_panics_and_yields_what_it_can() {
        let good = tag(
            4,
            0,
            &[
                frame(4, "TIT2", 3, b"Title"),
                frame(4, "TPE1", 3, b"Artist"),
            ],
        );
        for cut in 0..good.len() {
            let _ = parse_leading_tags(&good[..cut]);
        }
        assert_eq!(parse_leading_tags(b"ID3").display(), None);
        assert_eq!(parse_leading_tags(b"not a tag").display(), None);
        assert_eq!(parse_leading_tags(&[]).display(), None);

        let mut oversize = good.clone();
        oversize[10 + 4] = 0x7F; // frame size claims far more than the tag holds
        assert_eq!(parse_leading_tags(&oversize).title, None);
    }

    #[test]
    fn compressed_and_encrypted_frames_are_skipped() {
        let mut data = tag(
            4,
            0,
            &[frame(4, "TIT2", 3, b"Secret"), frame(4, "TPE1", 3, b"Open")],
        );
        data[10 + 9] = 0x08; // compression flag on the first frame

        let text = parse_leading_tags(&data);

        assert_eq!(text.title, None);
        assert_eq!(text.artist.as_deref(), Some("Open"));
    }

    #[test]
    fn display_covers_every_combination() {
        let both = Id3Text {
            title: Some("T".into()),
            artist: Some("A".into()),
        };
        let only_title = Id3Text {
            title: Some("T".into()),
            artist: None,
        };
        let only_artist = Id3Text {
            title: None,
            artist: Some("A".into()),
        };

        assert_eq!(both.display().as_deref(), Some("A - T"));
        assert_eq!(only_title.display().as_deref(), Some("T"));
        assert_eq!(only_artist.display().as_deref(), Some("A"));
    }

    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn arbitrary_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
                let _ = parse_leading_tags(&data);
            }

            #[test]
            fn corrupted_valid_tags_never_panic(
                title in "[ -~]{0,40}",
                flips in proptest::collection::vec((0usize..200, any::<u8>()), 0..8),
                version in prop_oneof![Just(3u8), Just(4u8)],
            ) {
                let mut data = tag(
                    version,
                    0,
                    &[frame(version, "TIT2", 3, title.as_bytes()), frame(version, "TPE1", 0, b"x")],
                );
                for (index, byte) in flips {
                    let at = index % data.len();
                    data[at] = byte;
                }
                let _ = parse_leading_tags(&data);
            }

            #[test]
            fn printable_titles_round_trip(title in "[A-Za-z0-9 ,.'-]{1,60}") {
                let data = tag(4, 0, &[frame(4, "TIT2", 3, title.as_bytes())]);
                let expected = title.trim();
                let parsed = parse_leading_tags(&data).title;

                if expected.is_empty() {
                    prop_assert_eq!(parsed, None);
                } else {
                    prop_assert_eq!(parsed.as_deref(), Some(expected));
                }
            }
        }
    }
}
