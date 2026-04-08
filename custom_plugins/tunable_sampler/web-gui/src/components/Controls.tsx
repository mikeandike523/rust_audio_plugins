import { RESAMPLE_OPTIONS } from "../constants";

type ControlsProps = {
  gain: number | null;
  onGainChange: (value: number) => void;
  resamplePointsInput: number | null;
  resamplePointsPitch: number | null;
  onResamplePointsInputChange: (value: number) => void;
  onResamplePointsPitchChange: (value: number) => void;
};

export const Controls = ({
  gain,
  onGainChange,
  resamplePointsInput,
  resamplePointsPitch,
  onResamplePointsInputChange,
  onResamplePointsPitchChange,
}: ControlsProps) => (
  <section className="controls">
    <div className="control">
      <label htmlFor="gain">Gain</label>
      <div className="control-row">
        <input
          id="gain"
          type="range"
          min="-24"
          max="24"
          step="0.1"
          value={gain ?? 0}
          onChange={(e) => onGainChange(Number(e.target.value))}
          disabled={gain === null}
        />
        <span className="value">{gain === null ? "—" : `${gain >= 0 ? "+" : ""}${gain.toFixed(1)} dB`}</span>
      </div>
    </div>

    <div className="control">
      <label htmlFor="resample-input">Resample · Project Match</label>
      <select
        id="resample-input"
        value={resamplePointsInput ?? RESAMPLE_OPTIONS[2]}
        onChange={(e) => onResamplePointsInputChange(Number(e.target.value))}
      >
        {RESAMPLE_OPTIONS.map((o) => (
          <option key={o} value={o}>{o} pts</option>
        ))}
      </select>
    </div>

    <div className="control">
      <label htmlFor="resample-pitch">Resample · Pitch Adjust</label>
      <select
        id="resample-pitch"
        value={resamplePointsPitch ?? RESAMPLE_OPTIONS[2]}
        onChange={(e) => onResamplePointsPitchChange(Number(e.target.value))}
      >
        {RESAMPLE_OPTIONS.map((o) => (
          <option key={o} value={o}>{o} pts</option>
        ))}
      </select>
    </div>
  </section>
);
