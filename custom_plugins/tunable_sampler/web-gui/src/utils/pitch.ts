const NOTE_NAMES = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"] as const;

export type NoteInfo = {
  hz: number;      // raw detected Hz
  noteHz: number;  // exact Hz of nearest 12-EDO pitch
  noteName: string;
  octave: number;
  cents: number;   // detected Hz deviation from nearest 12-EDO (positive = sharp)
};

export function hzToNoteInfo(hz: number): NoteInfo {
  const midiFloat = 69.0 + 12.0 * Math.log2(hz / 440.0);
  const midiRounded = Math.round(midiFloat);
  const cents = Math.round((midiFloat - midiRounded) * 100);
  const noteIndex = ((midiRounded % 12) + 12) % 12;
  const octave = Math.floor(midiRounded / 12) - 1;
  const noteHz = 440.0 * Math.pow(2, (midiRounded - 69) / 12);
  return { hz, noteHz, noteName: NOTE_NAMES[noteIndex], octave, cents };
}

/**
 * Format pitch for display.
 *
 * nudge = false (default): reference is the precise detected Hz.
 *   "441.2 Hz → A4 +4.7¢"
 *
 * nudge = true: reference is snapped to nearest 12-EDO pitch.
 *   "A4 (440.0 Hz) | err +4.7¢"
 */
export function formatPitchReadout(info: NoteInfo, nudge: boolean): string {
  const sign = info.cents >= 0 ? "+" : "";
  if (nudge) {
    return `${info.noteName}${info.octave} (${info.noteHz.toFixed(2)} Hz) | err ${sign}${info.cents}¢`;
  }
  return `${info.hz.toFixed(1)} Hz → ${info.noteName}${info.octave} ${sign}${info.cents}¢`;
}
