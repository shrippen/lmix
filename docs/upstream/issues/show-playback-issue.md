Title: Question: interest in a cue-based playback block (sound and picture cues in output zones)?

## Context

For theatre and live-event playback we built a block family on top of Strom: a Show Sound / Show Playback block that plays sound and picture cues into named output zones (up to 16 sound zones), with:
- cues that start, fade, stop, pause and resume per zone; a cue ends when its zones have played it, not when it has been decoded;
- seamless repeat of sounds and videos from one seam logic, with fades on the zone pads;
- each zone clocked by live silence, so a cue starts within a few milliseconds (5 ms silence blocks, 15 ms mixer latency);
- a mono file played on all channels of a zone.

It is large (several thousand lines, its own API) and close to a show-control application rather than a pipeline editor.

## Question for the maintainers

Is this something you would want in Strom? If yes, we would open a design issue first and then split it into PRs. If not, we keep it as our own block outside upstream, and only bring the generic parts upstream that it needed (each as its own issue or PR).

No code change is proposed by this issue.
