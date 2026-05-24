# Strum

A MIDI note effect that staggers simultaneous note-on events by consecutive multiples of a configurable delay, simulating a strummed chord.

## What it does

When multiple MIDI note-on events arrive at the exact same sample position (i.e. they were placed on the same tick in the DAW piano roll), Strum spreads them out in time by staggering each note by an additional *x* ms delay. Notes are sorted by pitch ascending (lowest to highest) before staggering, matching the natural motion of a guitar strum.

Example: a C major chord (C4, E4, G4) with stagger = 20 ms becomes:
- C4 fires at t = 0 ms
- E4 fires at t = 20 ms
- G4 fires at t = 40 ms

## Parameters

| Parameter | Range | Description |
|-----------|-------|-------------|
| Stagger   | 0 – 200 ms | Per-note delay increment. At 0 ms the plugin is transparent (no reordering). Automatable. |

## Behavior

- Strum only applies during **live playback or render** (transport running). During note preview (clicking in the piano roll while transport is stopped) events pass through unmodified.
- On playback stop, all queued/pending notes are immediately cancelled and a MIDI all-notes-off is emitted to avoid stuck notes.

## Formats

- **CLAP** (primary) — exported as a `NoteEffect`, no audio I/O required. Works natively in Reaper, Bitwig, and other CLAP hosts.
- **VST3** (secondary) — note that some hosts (notably Ableton Live) do not support VST3 MIDI-only effects and may refuse to load this format. Use CLAP when possible.

## Future / deferred features

- **Reverse note-off stagger**: When the note-offs that correspond to a strummed chord all land on the same tick, optionally stagger them in the reverse order (highest to lowest, or reverse of the original strum order) so the chord releases as naturally as it was triggered. This was omitted from the initial implementation to keep the first version simple.

- **Strum direction**: Currently always ascending (low → high). A direction toggle (ascending / descending / random) would be a natural addition.
