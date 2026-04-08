import { RESAMPLE_QUALITY_OPTIONS } from "../constants";

type ControlsProps = {
  gain: number | null;
  onGainChange: (value: number) => void;
  detune: number | null;
  onDetuneChange: (value: number) => void;
  onDetuneReset: () => void;
  resampleQualityInput: number | null;
  resampleQualityPitch: number | null;
  onResampleQualityInputChange: (value: number) => void;
  onResampleQualityPitchChange: (value: number) => void;
  onForceResample: () => void;
};

export const Controls = ({
  gain,
  onGainChange,
  detune,
  onDetuneChange,
  onDetuneReset,
  resampleQualityInput,
  resampleQualityPitch,
  onResampleQualityInputChange,
  onResampleQualityPitchChange,
  onForceResample,
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
      <label htmlFor="detune">Detune</label>
      <div className="control-row">
        {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions */}
        <div onDoubleClick={onDetuneReset} title="Double-click to reset">
          <input
            id="detune"
            type="range"
            min="-100"
            max="100"
            step="0.1"
            value={detune ?? 0}
            onChange={(e) => onDetuneChange(Number(e.target.value))}
            disabled={detune === null}
          />
        </div>
        <span className="value">{detune === null ? "—" : `${detune >= 0 ? "+" : ""}${detune.toFixed(1)}¢`}</span>
      </div>
    </div>

    <div className="control">
      <label htmlFor="resample-input">Resample · Rate Match</label>
      <div className="control-row">
        <select
          id="resample-input"
          value={resampleQualityInput ?? 2}
          onChange={(e) => onResampleQualityInputChange(Number(e.target.value))}
        >
          {RESAMPLE_QUALITY_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
        <button className="mini-button" type="button" onClick={onForceResample}>
          Force ↻
        </button>
      </div>
    </div>

    <div className="control">
      <label htmlFor="resample-pitch">Resample · Pitch Adjust</label>
      <select
        id="resample-pitch"
        value={resampleQualityPitch ?? 0}
        onChange={(e) => onResampleQualityPitchChange(Number(e.target.value))}
      >
        {RESAMPLE_QUALITY_OPTIONS.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
    </div>
  </section>
);
