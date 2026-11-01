Title: Mixers: start output at the first input buffer, and pin the audio mixer's sample rate

## What is missing

1. Inter coupling hands buffers over with the producer's running time. That only lines up when the coupled flows share base time 0 (direct media timing). The Vision Mixer's compositors and the Audio Mixer's aggregators start their output at running time zero, so with base time 0 they chase clock time from 0 and never use an input: program stays black, the mix stays silent.
2. A live Audio Mixer (`force_live`) that times out before its first input arrives fixates the default rate, 44.1 kHz. An input at 48 kHz that arrives later fails with not-negotiated: the channel strip converts the format, it does not resample.

## Proposed direction

1. A block property `start_time_selection` on both mixers: `zero` (default, today's behaviour) or `first`, which sets the aggregators' `start-time-selection` to `first`. With base time 0 that is clock time, which is what the coupled inputs carry. One shared module, applied to every compositor or bus aggregator of the block.
2. A construction-time property `sample_rate` on the Audio Mixer (Hz, 0 = unchanged): a capsfilter behind every aggregator (main, solo, monitor, aux, groups) and a resampler in every channel strip, so the buses run at that rate whatever the inputs deliver and whenever they start.

## Questions for the maintainers

1. Is a per-block property the right place, or should the mixers pick `first` themselves when the flow uses direct media timing?
2. Should `sample_rate` default to 48000 instead of 0 (a live mixer that starts unfed would then never land on 44.1 kHz)?

## Test case

- Two flows with base time 0, coupled through Inter Output / Inter Input into a Vision Mixer: with `start_time_selection=first` program shows the input; with `zero` it stays black.
- A live Audio Mixer with `sample_rate=48000`, started without input, then fed a 48 kHz source: it mixes; without the property it fails with not-negotiated. (We have `mixer_sample_rate_test` for this; both cases fail with the property ignored.)
