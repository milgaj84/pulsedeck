AC-3 / E-AC-3 HLS fixtures, all cut from the 1 s clear streams in
`src/audio/testdata/dolby/tone.ac3` and `tone.eac3` (stereo, 64 kbps, 48 kHz),
so the audio that comes out of each one must equal those files byte for byte.

```
ffmpeg -i tone.$C -c copy -f mpegts ts/atsc_$C.ts                       # stream type 0x81 / 0x87
ffmpeg -i tone.$C -c copy -mpegts_flags system_b -f mpegts ts/dvb_$C.ts  # private data (0x06) + descriptors
ffmpeg -i tone.$C -c copy -f hls -hls_time 2 -hls_segment_type fmp4 -hls_fmp4_init_filename init.mp4 \
  -hls_segment_filename 'fmp4/$C/seg%d.m4s' index.m3u8
```

`sample_aes/` is SAMPLE-AES from Shaka Packager v3.9.3 (key and IV as in
`../sample_aes/README.md`, one 2 s segment each):

```
ffmpeg -i tone.$C -c copy $C.mp4
packager "in=$C.mp4,stream=audio,segment_template=a_\$Number\$.ts,playlist_name=v.m3u8,format=ts" $K       # ts.ts
packager "in=$C.mp4,stream=audio,segment_template=a_\$Number\$.$C,playlist_name=v.m3u8" $K                 # packed.$C
packager "in=$C.mp4,stream=audio,init_segment=init.mp4,segment_template=s_\$Number\$.m4s,playlist_name=v.m3u8" $K  # cbcs
```

Checked independently: with the IV reset at every syncframe, the TS and packed
files decrypt to streams that ffmpeg decodes bit-exactly like the clear source
(ffmpeg's own HLS demuxer cannot read these two); ffmpeg decrypts the cbcs fMP4
files bit-exactly with `-decryption_key`.
