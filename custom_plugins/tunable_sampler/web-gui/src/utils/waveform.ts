import type { ChannelMode } from "../types/appTypes";

const WAVE_COLOR = "#e07a3f";
const MIDLINE_COLOR = "rgba(26, 26, 24, 0.18)";
const DIVIDER_COLOR = "rgba(26, 26, 24, 0.32)";
const LABEL_BG = "rgba(26, 26, 24, 0.6)";
const LABEL_FG = "#f4ece1";

export type WaveformDrawOptions = {
  channelMode: ChannelMode;
  /** Per-channel boosts applied before the routing/mix stage. */
  boostLDb: number;
  boostRDb: number;
  /** Global preamp (post-routing scalar); folded into both channels. */
  preampDb: number;
  /** Pre-computed (L+R)*0.5 mono mix, length = frame count. Only needed for MixMean. */
  mixMeanData: Float32Array | null;
};

const dbToGain = (db: number) => Math.pow(10, db / 20);

/**
 * Draw a single-channel waveform from `data` into the horizontal band
 * [yTop, yBottom] of the canvas, centered on the band's midline and scaled by
 * `gain`. Peaks are computed per output pixel so it stays crisp at any width —
 * which is what keeps the render seamless across container resizes.
 */
const drawChannelBand = (
  ctx: CanvasRenderingContext2D,
  data: Float32Array,
  frames: number,
  width: number,
  yTop: number,
  yBottom: number,
  gain: number,
) => {
  const mid = (yTop + yBottom) / 2;
  const padding = Math.min(8, (yBottom - yTop) / 4);
  const usableHeight = Math.max(0, (yBottom - yTop) / 2 - padding);
  const samplesPerPixel = Math.max(1, Math.floor(frames / width));

  ctx.strokeStyle = WAVE_COLOR;
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = 0; x < width; x += 1) {
    const start = x * samplesPerPixel;
    const end = Math.min(frames, start + samplesPerPixel);
    let peak = 0;
    for (let i = start; i < end; i += 1) {
      const abs = Math.abs(data[i]);
      if (abs > peak) peak = abs;
    }
    const amp = peak * usableHeight * gain;
    const xPos = x + 0.5;
    ctx.moveTo(xPos, mid - amp);
    ctx.lineTo(xPos, mid + amp);
  }
  ctx.stroke();

  ctx.strokeStyle = MIDLINE_COLOR;
  ctx.beginPath();
  ctx.moveTo(0, mid + 0.5);
  ctx.lineTo(width, mid + 0.5);
  ctx.stroke();
};

/** Small titlebar chip in the canvas top-left, naming the active channel mode. */
const drawLabel = (ctx: CanvasRenderingContext2D, text: string) => {
  ctx.font = "600 10px ui-monospace, 'SF Mono', Menlo, Consolas, monospace";
  ctx.textBaseline = "middle";
  const padX = 6;
  const w = ctx.measureText(text).width + padX * 2;
  const h = 15;
  ctx.fillStyle = LABEL_BG;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = LABEL_FG;
  ctx.fillText(text, padX, h / 2 + 0.5);
};

export const drawWaveform = (
  canvas: HTMLCanvasElement | null,
  audioBuffer: AudioBuffer | null,
  options: WaveformDrawOptions,
) => {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const width = canvas.width;
  const height = canvas.height;
  ctx.clearRect(0, 0, width, height);

  if (!audioBuffer) return;

  const frames = audioBuffer.length;
  const numCh = audioBuffer.numberOfChannels;
  const left = audioBuffer.getChannelData(0);
  const right = numCh > 1 ? audioBuffer.getChannelData(1) : left;

  const { channelMode, boostLDb, boostRDb, preampDb, mixMeanData } = options;
  const gL = dbToGain(boostLDb + preampDb);
  const gR = dbToGain(boostRDb + preampDb);

  switch (channelMode) {
    case 0: {
      // Stereo: L in the top band, R in the bottom band.
      drawChannelBand(ctx, left, frames, width, 0, height / 2, gL);
      drawChannelBand(ctx, right, frames, width, height / 2, height, gR);
      ctx.strokeStyle = DIVIDER_COLOR;
      ctx.beginPath();
      ctx.moveTo(0, height / 2 + 0.5);
      ctx.lineTo(width, height / 2 + 0.5);
      ctx.stroke();
      break;
    }
    case 1: {
      // MixMean: single waveform of the cached mono mix. When both preamps are
      // equal this scalar is exact; otherwise the average is a close visual
      // approximation (the cache itself is preamp-independent so dragging the
      // sliders never forces a recompute).
      const data = mixMeanData ?? left;
      const gMix = (gL + gR) / 2;
      drawChannelBand(ctx, data, frames, width, 0, height, gMix);
      drawLabel(ctx, "0.5L + 0.5R");
      break;
    }
    case 2: {
      drawChannelBand(ctx, left, frames, width, 0, height, gL);
      drawLabel(ctx, "Left");
      break;
    }
    case 3: {
      drawChannelBand(ctx, right, frames, width, 0, height, gR);
      drawLabel(ctx, "Right");
      break;
    }
  }
};
