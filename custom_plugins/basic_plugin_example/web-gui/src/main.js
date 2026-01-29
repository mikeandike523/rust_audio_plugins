import "./style.css";

const saturationSlider = document.getElementById("saturation");
const gainSlider = document.getElementById("gain");
const saturationValue = document.getElementById("saturation-value");
const gainValue = document.getElementById("gain-value");
const status = document.getElementById("status");

const meterInL = document.getElementById("meter-in-l");
const meterInR = document.getElementById("meter-in-r");
const meterOutL = document.getElementById("meter-out-l");
const meterOutR = document.getElementById("meter-out-r");

const sendToPluginSafe = (payload) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
  } else {
    console.log("sendToPlugin missing", payload);
  }
};

const setSaturationDisplay = (value) => {
  const clamped = Math.max(0, Math.min(10, value));
  saturationSlider.value = clamped.toFixed(2);
  saturationValue.textContent = `${clamped.toFixed(2)}x`;
};

const setGainDisplay = (value) => {
  const clamped = Math.max(-24, Math.min(24, value));
  gainSlider.value = clamped.toFixed(1);
  gainValue.textContent = `${clamped.toFixed(1)} dB`;
};

const setMeter = (element, value) => {
  const clamped = Math.max(0, Math.min(1, value));
  element.style.transform = `scaleY(${clamped})`;
};

saturationSlider.addEventListener("input", () => {
  const value = Number(saturationSlider.value);
  setSaturationDisplay(value);
  sendToPluginSafe({ type: "SetSaturation", value });
});

gainSlider.addEventListener("input", () => {
  const value = Number(gainSlider.value);
  setGainDisplay(value);
  sendToPluginSafe({ type: "SetGain", value });
});

window.onPluginMessage = (msg) => {
  if (msg.type === "ParamChange") {
    if (typeof msg.saturation === "number") {
      setSaturationDisplay(msg.saturation);
    }
    if (typeof msg.gain === "number") {
      setGainDisplay(msg.gain);
    }
    status.textContent = "Connected";
  }

  if (msg.type === "Meter") {
    setMeter(meterInL, msg.input?.l ?? 0);
    setMeter(meterInR, msg.input?.r ?? 0);
    setMeter(meterOutL, msg.output?.l ?? 0);
    setMeter(meterOutR, msg.output?.r ?? 0);
  }
};

sendToPluginSafe({ type: "Init" });
