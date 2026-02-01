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
      <input
        id="gain"
        type="range"
        min="-24"
        max="24"
        step="0.1"
        value={gain ?? 0}
        onChange={(event) => onGainChange(Number(event.target.value))}
        disabled={gain === null}
      />
      <div className="value">
        {gain === null ? "--" : `${gain.toFixed(1)} dB`}
      </div>
    </div>

    <div className="control">
      <label htmlFor="resample-input">Resample Points (Project Match)</label>
      <select
        id="resample-input"
        value={resamplePointsInput ?? RESAMPLE_OPTIONS[2]}
        onChange={(event) => onResamplePointsInputChange(Number(event.target.value))}
      >
        {RESAMPLE_OPTIONS.map((option) => (
          <option key={option} value={option}>
            {option} points
          </option>
        ))}
      </select>
      <div className="value">
        {resamplePointsInput === null ? "--" : `${resamplePointsInput} points`}
      </div>
    </div>

    <div className="control">
      <label htmlFor="resample-pitch">Resample Points (Pitch Adjust)</label>
      <select
        id="resample-pitch"
        value={resamplePointsPitch ?? RESAMPLE_OPTIONS[2]}
        onChange={(event) => onResamplePointsPitchChange(Number(event.target.value))}
      >
        {RESAMPLE_OPTIONS.map((option) => (
          <option key={option} value={option}>
            {option} points
          </option>
        ))}
      </select>
      <div className="value">
        {resamplePointsPitch === null ? "--" : `${resamplePointsPitch} points`}
      </div>
    </div>
  </section>
);
