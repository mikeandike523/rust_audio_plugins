import { useRef } from "react";
import type { ChannelMode, TuningStatus } from "../types/appTypes";
import { RESAMPLE_QUALITY_OPTIONS } from "../constants";

type ControlsProps = {
  channelMode: ChannelMode;
  boostL: number | null;
  onBoostLChange: (value: number) => void;
  onBoostLReset: () => void;
  boostR: number | null;
  onBoostRChange: (value: number) => void;
  onBoostRReset: () => void;
  preamp: number | null;
  onPreampChange: (value: number) => void;
  onPreampReset: () => void;
  gain: number | null;
  onGainChange: (value: number) => void;
  detune: number | null;
  onDetuneChange: (value: number) => void;
  onDetuneReset: () => void;
  attack: number | null;
  onAttackChange: (value: number) => void;
  decay: number | null;
  onDecayChange: (value: number) => void;
  sustain: number | null;
  onSustainChange: (value: number) => void;
  release: number | null;
  onReleaseChange: (value: number) => void;
  bendDepth: number | null;
  onBendDepthChange: (value: number) => void;
  polyphony: number | null;
  onPolyphonyChange: (value: number) => void;
  resampleQualityInput: number | null;
  resampleQualityPitch: number | null;
  onResampleQualityInputChange: (value: number) => void;
  onResampleQualityPitchChange: (value: number) => void;
  onForceResample: () => void;
  tuningStatus: TuningStatus | null;
  onSclFileChange: (file: File | null) => void;
  onKbmFileChange: (file: File | null) => void;
  onClearSclFile: () => void;
  onClearKbmFile: () => void;
};

export const Controls = ({
  channelMode,
  boostL,
  onBoostLChange,
  onBoostLReset,
  boostR,
  onBoostRChange,
  onBoostRReset,
  preamp,
  onPreampChange,
  onPreampReset,
  gain,
  onGainChange,
  detune,
  onDetuneChange,
  onDetuneReset,
  attack,
  onAttackChange,
  decay,
  onDecayChange,
  sustain,
  onSustainChange,
  release,
  onReleaseChange,
  bendDepth,
  onBendDepthChange,
  polyphony,
  onPolyphonyChange,
  resampleQualityInput,
  resampleQualityPitch,
  onResampleQualityInputChange,
  onResampleQualityPitchChange,
  onForceResample,
  tuningStatus,
  onSclFileChange,
  onKbmFileChange,
  onClearSclFile,
  onClearKbmFile,
}: ControlsProps) => {
  const sclInputRef = useRef<HTMLInputElement | null>(null);
  const kbmInputRef = useRef<HTMLInputElement | null>(null);
  const sclStatus = tuningStatus?.scl_name ? tuningStatus.scl_name : "No SCL loaded";
  const kbmStatus = tuningStatus?.kbm_name ? tuningStatus.kbm_name : "No KBM loaded";

  // The wrong-side boost is greyed out (but kept visible) in Left/Right modes.
  const leftDisabled = boostL === null || channelMode === 3;
  const rightDisabled = boostR === null || channelMode === 2;

  return (
  <section className="controls">
    <div className={`control${leftDisabled ? " is-disabled" : ""}`}>
      <label htmlFor="boost-l">LBoost</label>
      <div className="control-row">
        <div onDoubleClick={onBoostLReset} title="Left-channel boost (before mix). Double-click to reset to 0 dB">
          <input
            id="boost-l"
            type="range"
            min="-30"
            max="15"
            step="0.1"
            value={boostL ?? 0}
            onChange={(e) => onBoostLChange(Number(e.target.value))}
            disabled={leftDisabled}
          />
        </div>
        <span className="value">{boostL === null ? "—" : `${boostL >= 0 ? "+" : ""}${boostL.toFixed(1)} dB`}</span>
      </div>
    </div>

    <div className={`control${rightDisabled ? " is-disabled" : ""}`}>
      <label htmlFor="boost-r">RBoost</label>
      <div className="control-row">
        <div onDoubleClick={onBoostRReset} title="Right-channel boost (before mix). Double-click to reset to 0 dB">
          <input
            id="boost-r"
            type="range"
            min="-30"
            max="15"
            step="0.1"
            value={boostR ?? 0}
            onChange={(e) => onBoostRChange(Number(e.target.value))}
            disabled={rightDisabled}
          />
        </div>
        <span className="value">{boostR === null ? "—" : `${boostR >= 0 ? "+" : ""}${boostR.toFixed(1)} dB`}</span>
      </div>
    </div>

    <div className="control">
      <label htmlFor="preamp" title="Global preamp applied after the mix stage">Preamp</label>
      <div className="control-row">
        <div onDoubleClick={onPreampReset} title="Double-click to reset to 0 dB">
          <input
            id="preamp"
            type="range"
            min="-30"
            max="15"
            step="0.1"
            value={preamp ?? 0}
            onChange={(e) => onPreampChange(Number(e.target.value))}
            disabled={preamp === null}
          />
        </div>
        <span className="value">{preamp === null ? "—" : `${preamp >= 0 ? "+" : ""}${preamp.toFixed(1)} dB`}</span>
      </div>
    </div>

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
      <label htmlFor="bend-depth">Bend Depth</label>
      <div className="control-row">
        <input
          id="bend-depth"
          type="range"
          min="100"
          max="400"
          step="1"
          value={bendDepth ?? 200}
          onChange={(e) => onBendDepthChange(Number(e.target.value))}
          disabled={bendDepth === null}
        />
        <span className="value">{bendDepth === null ? "—" : `${Math.round(bendDepth)}¢`}</span>
      </div>
    </div>

    <div className="control">
      <label>Polyphony</label>
      <div className="radio-row">
        {[16, 24, 32].map((voices) => (
          <label key={voices} className="radio-chip">
            <input
              type="radio"
              name="polyphony"
              checked={polyphony === voices}
              onChange={() => onPolyphonyChange(voices)}
            />
            <span>{voices}</span>
          </label>
        ))}
      </div>
    </div>

    <div className="control">
      <label htmlFor="attack">Attack</label>
      <div className="control-row">
        <input
          id="attack"
          type="range"
          min="0"
          max="5"
          step="0.001"
          value={attack ?? 0.01}
          onChange={(e) => onAttackChange(Number(e.target.value))}
          disabled={attack === null}
        />
        <span className="value">{attack === null ? "—" : `${attack.toFixed(3)} s`}</span>
      </div>
    </div>

    <div className="control">
      <label htmlFor="decay">Decay</label>
      <div className="control-row">
        <input
          id="decay"
          type="range"
          min="0"
          max="5"
          step="0.001"
          value={decay ?? 0.1}
          onChange={(e) => onDecayChange(Number(e.target.value))}
          disabled={decay === null}
        />
        <span className="value">{decay === null ? "—" : `${decay.toFixed(3)} s`}</span>
      </div>
    </div>

    <div className="control">
      <label htmlFor="sustain">Sustain</label>
      <div className="control-row">
        <input
          id="sustain"
          type="range"
          min="0"
          max="1"
          step="0.001"
          value={sustain ?? 1}
          onChange={(e) => onSustainChange(Number(e.target.value))}
          disabled={sustain === null}
        />
        <span className="value">{sustain === null ? "—" : sustain.toFixed(3)}</span>
      </div>
    </div>

    <div className="control">
      <label htmlFor="release">Release</label>
      <div className="control-row">
        <input
          id="release"
          type="range"
          min="0"
          max="10"
          step="0.001"
          value={release ?? 0.25}
          onChange={(e) => onReleaseChange(Number(e.target.value))}
          disabled={release === null}
        />
        <span className="value">{release === null ? "—" : `${release.toFixed(3)} s`}</span>
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

    <div className="control tuning-control">
      <label>Tuning</label>
      <div className="tuning-file-row">
        <input
          ref={sclInputRef}
          className="hidden-file-input"
          type="file"
          accept=".scl"
          onChange={(e) => {
            onSclFileChange(e.target.files?.[0] ?? null);
            e.target.value = "";
          }}
        />
        <button
          className="file-trigger"
          type="button"
          onClick={() => sclInputRef.current?.click()}
        >
          Choose SCL
        </button>
        <button className="mini-button" type="button" onClick={onClearSclFile}>Clear SCL</button>
        <div className="file-status" title={sclStatus}>{sclStatus}</div>
      </div>
      <div className="tuning-file-row">
        <input
          ref={kbmInputRef}
          className="hidden-file-input"
          type="file"
          accept=".kbm"
          onChange={(e) => {
            onKbmFileChange(e.target.files?.[0] ?? null);
            e.target.value = "";
          }}
        />
        <button
          className="file-trigger"
          type="button"
          onClick={() => kbmInputRef.current?.click()}
        >
          Choose KBM
        </button>
        <button className="mini-button" type="button" onClick={onClearKbmFile}>Clear KBM</button>
        <div className="file-status" title={kbmStatus}>{kbmStatus}</div>
      </div>
      <div className={`control-meta${tuningStatus?.error ? " control-meta-error" : ""}`}>
        {tuningStatus?.error
          ? tuningStatus.error
          : tuningStatus?.active
            ? "Tuning active"
            : "Using default 12-EDO"}
      </div>
    </div>
  </section>
  );
};
