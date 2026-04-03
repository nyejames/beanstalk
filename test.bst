volt_drop_per_km = 11.0
km_normalized = 1000.0
mv_normalized = 1000.0
length = 25.0
required_load = 125.0
ze = 0.16

r1 = (volt_drop_per_km * length * required_load) / mv_normalized
r2 = r1 / km_normalized -- Assumes that the resistance is the same
zs = ze + (r1 / km_normalized) + r2
three_phase = true

if three_phase:
    result = zs * 2 -- Fault current should be assumed to be 2x the voltage of a single phase 
    io(result)
else
    io(zs)
;