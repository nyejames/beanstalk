-- Float range with explicit by step
samples ~= 0
loop t in 0.0 to 1.0 by 0.2:
    io([: t= t])
    samples = samples + 1
;
io([: float_samples= samples])
