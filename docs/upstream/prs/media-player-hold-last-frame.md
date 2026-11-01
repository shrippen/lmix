Title: fix(media player): hold the last frame while paused

## Problem

Pausing a Media Player stops its internal pipeline, so the main pipeline's video appsrc gets no more buffers. A live aggregator downstream (a Vision Mixer input, also across Inter Output / Inter Input) then counts the pad as active but starving and waits for its deadline on every output frame. One paused clip anywhere on a mixer, for example a clip parked for its next use, added about 30 ms to program latency (86 instead of 56 ms, measured on a GPU mixer).

## Change

- While paused, and at the end of the playlist, the player repeats its last video frame at the frame rate (40 ms when the caps carry none), stamped with the main pipeline's running time (new module `mediaplayer/still.rs`). The bridge stores the last sample of the first video track as it pushes it.
- Only raw video (`video/x-raw`, any memory feature) is held. A player that passes its file through (decode off) carries encoded frames, mostly delta frames; repeated with a fresh PTS and no DTS they would break a decoder or muxer downstream, so such a player sends nothing while paused, as before.
- Play, resume and file switches end the repeat and forget the frame, so a pause on a new file before its first frame holds nothing rather than the previous file's frame, and the buffer goes back to its pool. Next, previous and goto play the new file, so the output stays fed. A stinger clip source parks as before and does not repeat.
- At the end of the playlist the hold starts under the player's control lock, where the EOS handler already checks the switch generation and the pause, so a play or file switch that comes in meanwhile runs after it and ends it. A seek on a player that is not paused (a finished playlist played again) ends the hold too; a paused or stopped player keeps its frame.
- The repeat upgrades its weak references to the appsrc and the main pipeline only around each push, under a lock that `release()` waits for, and `shutdown()` releases it before the flow is dropped, so removing a paused player's flow does not keep the pipeline alive (the lifecycle leak check stays clean).

Rejected: `ignore-inactive-pads` on the consumers (only skips pads that never had a buffer; a paused player's pad had buffers).

## Evidence

- A paused player keeps feeding its output: `backend/tests/media_player_control_test.rs::a_paused_player_keeps_feeding_its_output` plays a clip through a real flow, pauses it and counts buffers at the flow's sink for 2 s. With the hold removed from `pause()`: "the paused player fed 0 frames in 2 s" (2 of 2 runs); with it, the full rate.
- The repeat lets go of the pipeline: `mediaplayer::still::tests::release_lets_go_of_pipeline` fails when the repeat keeps its strong references across the sleep ("pipeline still referenced").
- Passthrough: `a_paused_passthrough_player_repeats_no_encoded_frame` (decode off, pause, count buffers for 2 s) and `still::tests::encoded_frames_are_not_held` fail when encoded samples are stored ("the paused passthrough player repeated frames").
- A file switch forgets the frame: `a_player_paused_on_a_new_file_does_not_show_the_old_one` (play a video, go to an audio-only file, pause) and `still::tests::release_forgets_the_frame` fail when `release()` keeps the frame ("the paused player showed the old file's frame").
- `a_paused_player_switching_files_keeps_feeding_its_output`: next, previous and goto while paused each feed the output at the full rate. It passes without the fixes too: it guards existing behaviour (these commands always play the new file).
- `still::tests::hold_repeats_until_release` and `frame_interval_from_caps` cover the repeat itself.
- Seek after the end: `a_seek_after_the_end_of_the_playlist_ends_the_hold` (one 2 s clip, no loop, wait past the end, seek to 0, count buffers for 2 s) fails when the seek keeps the hold ("after the seek the player fed 100 frames in 2 s").
- The end-of-playlist race (a control call between the EOS handler's unlock and its hold) has no deterministic test; holding under the lock removes that window.
- The latency figure is from a manual run, not reproduced by a test here.

## Tests

Ran on Linux (Fedora 44, GStreamer 1.28.8, x86_64), on current `main` (4479d8c):
- `cargo test --lib mediaplayer`: 46 passed.
- `cargo test --test media_player_control_test`: 6 passed (the storm test and the five new ones).
- `cargo test --test pipeline_lifecycle_test`: 4 passed.
- `cargo fmt --all --check`: clean.
- `cargo clippy --all-targets`: no warnings in the touched files (three pre-existing warnings from a newer clippy in untouched files).

## Out of scope

- Audio: the audio appsrcs still go silent while paused, so a live `audiomixer` downstream waits for its timeout as before.
- Each hold starts its own thread; an old one ends after its next sleep (at most one frame interval), so a quick pause/play/pause briefly has two, of which only the newest pushes.

## For the reviewer

- A seek while paused keeps showing the old frame: no preroll frame is pushed after the seek.

Behaviour change: a paused or finished player now shows its last frame continuously instead of stopping its output. That is what a downstream mixer expects from a paused source (as with a paused player on a hardware mixer); a consumer that relied on the output going silent on pause would see frames. In particular:
- The paused player's branch runs at the full frame rate again (conversion, scaling, upload at a mixer input, an encoder behind it), per paused player. The repeat itself is cheap (one wake-up per frame, a shallow buffer copy); the cost is downstream, about that of playback, and adds up with many paused players compared with zero frames today.
- Stop (pause and seek to the start) now shows the frame it stopped on, not nothing.
- A recorder behind a paused player keeps writing the still frame; the file grows while paused.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
