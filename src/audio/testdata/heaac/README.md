Synthetic HE-AAC test streams (ADTS), made from generated audio only (no
recordings): 2 s of a 1 kHz tone plus white noise high-passed at 12 kHz, stereo,
44.1 kHz. The noise sits above the 11.025 kHz Nyquist limit of the AAC-LC core,
so it exists in the decoded output only if the SBR layer is decoded.

- `he_v1.aac`  HE-AAC v1 (SBR), 48 kbit/s
- `he_v2.aac`  HE-AAC v2 (SBR + Parametric Stereo), 32 kbit/s, a mono core

Encoded with the upstream Fraunhofer FDK-AAC example encoder (`aac-enc`, built
from https://github.com/mstorsjo/fdk-aac, commit c41c3c7), used here only as a
test-data generator; none of that code is part of PulseDeck:

```
ffmpeg -filter_complex "sine=f=1000:d=2:r=44100[a];anoisesrc=d=2:r=44100:amplitude=0.3:seed=7,highpass=f=12000[b];[a][b]amix=inputs=2:normalize=0,aformat=channel_layouts=stereo" -c:a pcm_s16le src.wav
aac-enc -t 5  -r 48000 src.wav he_v1.aac
aac-enc -t 29 -r 32000 src.wav he_v2.aac
```
