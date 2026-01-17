-- =====================================
-- Harmonic ADSR builder (simplified)
-- =====================================
-- Default ADSR parameters:
local baseAmpDb = -6.0 -- peak of 1st harmonic (dB)
local ampStepDb = -3.0 -- drop per harmonic (dB)
local baseSusDb = -12.0 -- sustain of 1st harmonic (dB)
local susStepDb = -2.0 -- drop per harmonic (dB)
local attackSeconds = 0.005 -- constant attack time
local decaySeconds = 0.1 -- constant decay time
local baseRelSec = 0.5 -- release of 1st harmonic (s)
local relStepSec = -0.05 -- change per harmonic (s)

-- Helper functions (all local):
local function calcPeakDb(n)
    return baseAmpDb + (n - 1) * ampStepDb
end

local function calcSustainDb(n)
    return baseSusDb + (n - 1) * susStepDb
end

local function calcReleaseSec(n)
    local r = baseRelSec + (n - 1) * relStepSec
    return (r > 0) and r or 0
end

local function dBToAmp(db)
    return 10 ^ (db / 20)
end

--- build(n) -> table
-- Returns a table keyed 1..n, each entry an ADSR sub‐table.
function build(n)
    assert(type(n) == "number" and n >= 1, "build: n must be a positive integer")
    local envs = {}
    for i = 1, n do
        envs[tostring(i)] = {
            v = dBToAmp(calcPeakDb(i)), -- peak amplitude (amplitude, NOT dB)
            a = attackSeconds, -- attack time (s)
            d = decaySeconds, -- decay time (s)
            s = dBToAmp(calcSustainDb(i)), -- sustain level (amplitude, NOT dB)
            r = calcReleaseSec(i) -- release time (s)
        }
    end
    return envs
end

return build(6)
