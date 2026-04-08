const NOTE_NAMES = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"] as const;

export type NoteInfo = {
  hz: number;
  noteName: string;
  octave: number;
  cents: number;
};

export function hzToNoteInfo(hz: number): NoteInfo {
  const midiFloat = 69.0 + 12.0 * Math.log2(hz / 440.0);
  const midiRounded = Math.round(midiFloat);
  const cents = Math.round((midiFloat - midiRounded) * 100);
  const noteIndex = ((midiRounded % 12) + 12) % 12;
  const octave = Math.floor(midiRounded / 12) - 1;
  return { hz, noteName: NOTE_NAMES[noteIndex], octave, cents };
}

export function formatPitchReadout(info: NoteInfo): string {
  const sign = info.cents >= 0 ? "+" : "";
  return `${info.hz.toFixed(1)} Hz → ${info.noteName}${info.octave} ${sign}${info.cents}¢`;
}
