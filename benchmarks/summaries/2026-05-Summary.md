# May 2026 Summary

## End-to-end CLI / macOS Apple Silicon (6D851D)
Change since initial benchmark: case set changed: avg +8ms on 8/16 shared cases; 5 slower, 2 faster
Initial: all ~37ms, core ~82ms, docs ~87ms, stress ~8ms
Latest: all ~29ms, core ~63ms, docs ~142ms, stress ~19ms, module ~11ms, borrow ~12ms
Case spread latest: ~34ms

## Frontend phases / macOS Apple Silicon (6D851D)
Change since initial benchmark: case set changed: avg +3ms on 7/8 shared cases; 2 slower, 0 faster
Initial: all ~80ms, core ~54ms, docs ~334ms, stress ~57ms, module ~22ms, borrow ~16ms
Latest: all ~74ms, core ~55ms, docs ~343ms, stress ~60ms, module ~18ms, borrow ~18ms
Case spread latest: ~104ms
---------------------

# End-to-end CLI / macOS Apple Silicon (6D851D): May 11th - 10:09
**baseline**; 8 cases
Avg: all ~37ms, core ~82ms, docs ~87ms, stress ~8ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 15th - 01:03
no measurable change since last benchmark

# End-to-end CLI / macOS Apple Silicon (6D851D): May 16th - 15:41
no measurable change: avg +3ms; 11/11 cases
Avg: all ~39ms, core ~111ms, docs ~127ms, stress ~11ms, module ~10ms, borrow ~10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 16th - 19:49
**-3ms avg**; 2 faster, 0 slower; 11/11 cases
Avg: all ~36ms, core ~103ms, docs ~111ms, stress ~10ms, module ~9ms, borrow ~9ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 17th - 03:08
no measurable change: avg 0ms; 11/11 cases
Avg: all ~37ms, core ~108ms, docs ~106ms, stress ~10ms, module ~10ms, borrow ~11ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 17th - 03:48
**0ms avg**; 1 faster, 0 slower; 11/11 cases
Avg: all ~37ms, core ~107ms, docs ~111ms, stress ~10ms, module ~10ms, borrow ~10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 17th - 04:39
no measurable change: avg -2ms; 11/11 cases
Avg: all ~35ms, core ~104ms, docs ~105ms, stress ~10ms, module ~9ms, borrow ~9ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 17th - 04:52
**0ms avg**; 1 faster, 0 slower; 11/11 cases
Avg: all ~36ms, core ~105ms, docs ~109ms, stress ~9ms, module ~9ms, borrow ~11ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 17th - 11:04
no measurable change: avg +3ms; 11/11 cases
Avg: all ~38ms, core ~107ms, docs ~125ms, stress ~10ms, module ~10ms, borrow ~10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 18th - 22:22
mixed: avg -13ms; 2 faster, 2 slower; 11/11 cases
Avg: all ~25ms, core ~30ms, docs ~107ms, stress ~16ms, module ~9ms, borrow ~10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 19th - 22:40
no measurable change: avg -1ms; 11/11 cases
Avg: all ~25ms, core ~30ms, docs ~105ms, stress ~16ms, module ~9ms, borrow ~10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 21st - 08:11
**-2ms avg**; 3 faster, 0 slower; 14/14 cases
Avg: all ~24ms, core ~32ms, docs ~110ms, stress ~18ms, module ~11ms, borrow ~12ms
Stage movement: ast -37ms, ast emit -23ms, ast env -11ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 22nd - 21:37
**-8ms avg**; 10 faster, 0 slower; 14/14 cases
Avg: all ~38ms, core ~118ms, docs ~133ms, stress ~18ms, module ~12ms, borrow ~11ms
Stage movement: ast -109ms, ast emit -71ms, file prep -36ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 24th - 07:08
no measurable change: avg 0ms; 14/14 cases
Avg: all ~38ms, core ~118ms, docs ~136ms, stress ~18ms, module ~12ms, borrow ~11ms
Stage movement: ast emit +6ms, file prep -5ms, ast env -4ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 24th - 14:02
**+2ms avg**; 0 faster, 2 slower; 14/14 cases
Avg: all ~40ms, core ~123ms, docs ~136ms, stress ~19ms, module ~13ms, borrow ~12ms
Stage movement: ast +59ms, ast env +34ms, file prep -32ms

# Frontend phases / macOS Apple Silicon (6D851D): May 31st - 08:35
**baseline**; 7 cases
Avg: all ~80ms, core ~54ms, docs ~334ms, stress ~57ms, module ~22ms, borrow ~16ms

# Frontend phases / macOS Apple Silicon (6D851D): May 31st - 08:58
**+7ms avg**; 0 faster, 1 slower; 7/7 cases
Avg: all ~88ms, core ~55ms, docs ~375ms, stress ~60ms, module ~23ms, borrow ~18ms
Stage movement: ast +12ms, ast env -3ms, ast emit +2ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 31st - 09:24
**-5ms avg**; 2 faster, 0 slower; 14/14 cases
Avg: all ~35ms, core ~76ms, docs ~153ms, stress ~21ms, module ~13ms, borrow ~12ms
Stage movement: ast +73ms, ast emit +57ms, hir -48ms

# Frontend phases / macOS Apple Silicon (6D851D): May 31st - 09:24
no measurable change: avg -4ms; 7/7 cases
Avg: all ~84ms, core ~54ms, docs ~340ms, stress ~59ms, module ~27ms, borrow ~20ms
Stage movement: ast -5ms, ast emit -5ms, ast env -1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 31st - 09:56
case set changed: avg 0ms on 14/16 shared cases; 0 slower, 0 faster
Avg: all ~32ms, core ~78ms, docs ~149ms, stress ~21ms, module ~12ms, borrow ~12ms
Stage movement: ast -34ms, ast emit -27ms, ast finalize -5ms

# Frontend phases / macOS Apple Silicon (6D851D): May 31st - 09:56
case set changed: avg -1ms on 7/8 shared cases; 0 slower, 0 faster
Avg: all ~74ms, core ~55ms, docs ~343ms, stress ~60ms, module ~18ms, borrow ~18ms
Stage movement: file prep -2ms, ast emit +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): May 31st - 13:38
**-3ms avg**; 2 faster, 0 slower; 16/16 cases
Avg: all ~29ms, core ~63ms, docs ~142ms, stress ~19ms, module ~11ms, borrow ~12ms
Stage movement: ast -70ms, ast emit -54ms, ast env -13ms

