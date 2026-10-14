Fixtures for the SAMPLE-AES tests. Key `000102030405060708090a0b0c0d0e0f`,
IV `0x0123456789abcdef0123456789abcdef`, `clear.aac` is the audio before
encryption (4 s sine, AAC-LC mono 44.1 kHz).

Made with Shaka Packager v3.9.3 (a real, independent producer):

```
ffmpeg -f lavfi -i sine=f=440:d=4:sample_rate=44100 -ac 1 -c:a aac -b:a 32k -f adts clear.aac
ffmpeg -i clear.aac -c copy tone.m4a
K="--enable_raw_key_encryption --keys label=:key_id=00112233445566778899aabbccddeeff:key=000102030405060708090a0b0c0d0e0f \
   --iv 0123456789abcdef0123456789abcdef --protection_scheme cbcs --hls_key_uri key.bin --clear_lead 0 --segment_duration 1"
packager 'in=tone.m4a,stream=audio,segment_template=ts/a_$Number$.ts,playlist_name=ts.m3u8,format=ts' $K
packager 'in=tone.m4a,stream=audio,segment_template=packed/a_$Number$.aac,playlist_name=packed.m3u8' $K
packager 'in=tone.m4a,stream=audio,init_segment=cbcs/init.mp4,segment_template=cbcs/seg_$Number$.m4s,playlist_name=cbcs.m3u8' $K
```

- `ts/`     MPEG-TS, AAC with stream type 0xCF
- `packed/` packed audio (ID3 + ADTS)
- `cbcs/`   fragmented MP4, `cbcs` protection (init + segments)

ffmpeg 8 decrypts all three to the same PCM as `clear.aac` (bit-exact), e.g. for
the fMP4: `cat init.mp4 seg_*.m4s > all.mp4; ffmpeg -decryption_key 000102..0f -i all.mp4 -f s16le x.pcm`.
For TS and packed audio, the playlist is read with
`ffmpeg -allowed_extensions ALL -protocol_whitelist file,crypto,data -i x.m3u8`.
