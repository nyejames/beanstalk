-- Float range with explicit by step
samples ~= 0
loop 0.0 to 1.0 by 0.2 t:
    samples = samples + 1
;
io([: float_samples = [samples] ])
