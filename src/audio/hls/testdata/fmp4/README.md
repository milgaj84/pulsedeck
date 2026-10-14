Fixtures for the fMP4/CMAF HLS tests (made with ffmpeg 8):

- `aac/`  fragmented MP4 AAC-LC, mono 44.1 kHz, init + 5 segments
- `ts/`   the same AAC frames as MPEG-TS segments (the reference for `aac/`)
- `opus/` fragmented MP4 Opus, stereo 48 kHz, init + 4 segments

```
ffmpeg -f lavfi -i sine=f=440:d=4:sample_rate=44100 -ac 1 -c:a aac -b:a 32k -f adts src.aac
ffmpeg -i src.aac -c:a copy -bsf:a aac_adtstoasc -f hls -hls_time 1 -hls_playlist_type vod \
  -hls_segment_type fmp4 -hls_fmp4_init_filename init.mp4 -hls_segment_filename 'aac/seg%d.m4s' aac/index.m3u8
ffmpeg -i src.aac -c:a copy -f hls -hls_time 1 -hls_playlist_type vod \
  -hls_segment_type mpegts -hls_segment_filename 'ts/seg%d.ts' ts/index.m3u8
ffmpeg -f lavfi -i sine=f=440:d=3:sample_rate=48000 -ac 2 -c:a libopus -b:a 32k -f hls -hls_time 1 \
  -hls_playlist_type vod -hls_segment_type fmp4 -hls_fmp4_init_filename init.mp4 \
  -hls_segment_filename 'opus/seg%d.m4s' opus/index.m3u8
```
