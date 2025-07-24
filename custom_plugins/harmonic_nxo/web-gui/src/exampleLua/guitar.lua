-- =====================================
-- Harmonic ADSR builder (simplified)
-- =====================================

-- Default ADSR parameters:
local baseAmpDb      = -6.0    -- peak of 1st harmonic (dB)
local ampStepDb      = -3.0    -- drop per harmonic (dB)
local baseSusDb      = -12.0   -- sustain of 1st harmonic (dB)
local susStepDb      = -2.0    -- drop per harmonic (dB)
local attackSeconds  = 0.005   -- constant attack time
local decaySeconds   = 0.1     -- constant decay time
local baseRelSec     = 0.5     -- release of 1st harmonic (s)
local relStepSec     = -0.05   -- change per harmonic (s)

-- Helper functions (all local):
local function calcPeakDb(n)
  return baseAmpDb + (n-1) * ampStepDb
end

local function calcSustainDb(n)
  return baseSusDb + (n-1) * susStepDb
end

local function calcReleaseSec(n)
  local r = baseRelSec + (n-1) * relStepSec
  return (r > 0) and r or 0
end

--- build(n) -> table
-- Returns a table keyed 1..n, each entry an ADSR sub‐table.
function build(n)
  assert(type(n)=="number" and n>=1, "build: n must be a positive integer")
  local envs = {}
  for i = 1, n do
    envs[i] = {
      amplitudeDb        = calcPeakDb(i),
      attackSeconds      = attackSeconds,
      decaySeconds       = decaySeconds,
      sustainAmplitudeDb = calcSustainDb(i),
      releaseSeconds     = calcReleaseSec(i),
    }
  end
  return envs
end

return build(6)