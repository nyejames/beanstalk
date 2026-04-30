import @core/math {sin, cos, pow, clamp}
import @core/time/now_millis
import @core/random/random_float

angle = 1.0
s = sin(angle)
c = cos(angle)
p = pow(2.0, 3.0)
cl = clamp(5.0, 0.0, 10.0)
m = now_millis()
r = random_float()
io(s)
io(c)
io(p)
io(cl)
io(m)
io(r)
