export const drawWaveform = (
  canvas: HTMLCanvasElement | null,
  audioBuffer: AudioBuffer | null,
  preampDb = 0,
) => {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const width = canvas.width;
  const height = canvas.height;

  ctx.clearRect(0, 0, width, height);

  if (!audioBuffer) {
    return;
  }

  const frames = audioBuffer.length;
  const channels = audioBuffer.numberOfChannels;
  const mid = height / 2;
  const padding = 12;
  const usableHeight = Math.max(0, mid - padding);
  const preampScale = Math.pow(10, preampDb / 20);

  ctx.strokeStyle = "#e07a3f";
  ctx.lineWidth = 1;
  ctx.beginPath();

  const samplesPerPixel = Math.max(1, Math.floor(frames / width));
  for (let x = 0; x < width; x += 1) {
    const start = x * samplesPerPixel;
    const end = Math.min(frames, start + samplesPerPixel);
    let peak = 0;
    for (let ch = 0; ch < channels; ch += 1) {
      const data = audioBuffer.getChannelData(ch);
      for (let i = start; i < end; i += 1) {
        const abs = Math.abs(data[i]);
        if (abs > peak) peak = abs;
      }
    }
    const amp = peak * usableHeight * preampScale;
    const xPos = x + 0.5;
    ctx.moveTo(xPos, mid - amp);
    ctx.lineTo(xPos, mid + amp);
  }

  ctx.stroke();

  ctx.strokeStyle = "rgba(26, 26, 24, 0.18)";
  ctx.beginPath();
  ctx.moveTo(0, mid + 0.5);
  ctx.lineTo(width, mid + 0.5);
  ctx.stroke();
};
