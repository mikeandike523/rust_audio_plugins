import { forwardRef, useMemo, useRef, type RefObject } from "react";
import { Div, type DivProps } from "style-props-html";
import useMonitorSize from "../utils/useMonitorSize";

export interface PianoWidgetProps extends DivProps {
  /** Length-128 boolean array (MIDI 0–127) telling which keys are held */
  midiStates: boolean[];
}

/** White keys are usually about 8× taller than they are wide */
const WHITE_KEY_ASPECT_RATIO = 1 / 8;
const FIRST_MIDI = 21;  // A0
const LAST_MIDI = 108;  // C8
const TOTAL_PIANO_NOTES = LAST_MIDI - FIRST_MIDI + 1; // 88
const BLACK_NOTE_OFFSETS = new Set([1, 3, 6, 8, 10]); // semitone offsets that are black

const isBlack = (midi: number) => BLACK_NOTE_OFFSETS.has(midi % 12);

export default forwardRef<HTMLDivElement, PianoWidgetProps>(function PianoWidget(
  { midiStates, ...rest },
  ref
) {
  const innerRef = useRef<HTMLDivElement | null>(null);
  const populatedRef = (ref as RefObject<HTMLDivElement>) || innerRef;
  const { width: containerWidth = 0 } =
    useMonitorSize(populatedRef as RefObject<HTMLDivElement>) ?? {
      width: 0,
      height: 0,
    };

  // Precompute geometry
  const { whiteKeys, blackKeys, whiteKeyHeight } = useMemo(() => {
    // White keys count (A0–C8 range has 52 white keys)
    const whiteMidiNotes: number[] = [];
    const blackMidiNotes: number[] = [];
    for (let m = FIRST_MIDI; m <= LAST_MIDI; m++) {
      (isBlack(m) ? blackMidiNotes : whiteMidiNotes).push(m);
    }

    const whiteKeyWidth = containerWidth / whiteMidiNotes.length;
    const whiteKeyHeight = whiteKeyWidth / WHITE_KEY_ASPECT_RATIO;

    // Map MIDI -> index of the preceding white key
    const whiteIndexMap: Record<number, number> = {};
    let wIdx = 0;
    for (let m = FIRST_MIDI; m <= LAST_MIDI; m++) {
      if (!isBlack(m)) {
        whiteIndexMap[m] = wIdx;
        wIdx++;
      } else {
        // black notes "belong" to the white key just before them
        whiteIndexMap[m] = wIdx - 1;
      }
    }

    const whiteKeys = whiteMidiNotes.map((m) => ({
      midi: m,
      left: whiteIndexMap[m] * whiteKeyWidth,
      width: whiteKeyWidth,
      height: whiteKeyHeight,
    }));

    const blackWidth = whiteKeyWidth * 0.6;
    const blackHeight = whiteKeyHeight * 0.6;

    const blackKeys = blackMidiNotes.map((m) => ({
      midi: m,
      left: whiteIndexMap[m] * whiteKeyWidth + whiteKeyWidth - blackWidth / 2,
      width: blackWidth,
      height: blackHeight,
    }));

    return { whiteKeys, blackKeys, whiteKeyHeight };
  }, [containerWidth]);

  return (
    <Div
      ref={populatedRef}
      width="100%"
      height={`${whiteKeyHeight}px`}
      position="relative"
      background="white"
      {...rest}
    >
      {/* White keys first (background layer) */}
      {whiteKeys.map(({ midi, left, width, height }) => {
        const pressed = midiStates[midi] === true;
        return (
          <Div
            key={midi}
            style={{
              position: "absolute",
              left,
              top: 0,
              width,
              height,
              boxSizing: "border-box",
              background: pressed ? "#8fd3ff" : "#fff",
              border: "1px solid #333",
              borderBottomLeftRadius: 2,
              borderBottomRightRadius: 2,
            }}
          />
        );
      })}

      {/* Black keys on top */}
      {blackKeys.map(({ midi, left, width, height }) => {
        const pressed = midiStates[midi] === true;
        return (
          <Div

            key={midi}
            style={{
              position: "absolute",
              left,
              top: 0,
              width,
              height,
              boxSizing: "border-box",
              background: pressed ? "#4aa3ff" : "#000",
              border: "1px solid #111",
              borderRadius: 2,
              zIndex: 2,
            }}
          />
        );
      })}
    </Div>
  );
});
