# June 2026 Summary

## End-to-end CLI / macOS Apple Silicon (6D851D)
Change since initial benchmark: case set changed: avg -22ms on 16/25 shared cases; 1 slower, 14 faster
Initial: all ~46ms, core ~89ms, docs ~224ms, stress ~32ms, module ~21ms, borrow ~19ms
Latest: all ~20ms, core ~10ms, docs ~143ms, stress ~17ms, module ~12ms, borrow ~15ms
Case spread latest: ~26ms

## Frontend phases / macOS Apple Silicon (6D851D)
Change since initial benchmark: case set changed: avg -45ms on 8/16 shared cases; 0 slower, 8 faster
Initial: all ~124ms, core ~134ms, docs ~423ms, stress ~119ms, module ~51ms, borrow ~45ms
Latest: all ~55ms, core ~64ms, docs ~325ms, stress ~39ms, module ~27ms, borrow ~28ms
Case spread latest: ~72ms
---------------------

# End-to-end CLI / macOS Apple Silicon (6D851D): June 3rd - 13:29
no measurable change: avg 0ms; 16/16 cases
Avg: all ~46ms, core ~90ms, docs ~226ms, stress ~32ms, module ~20ms, borrow ~19ms
Stage movement: ast -16ms, ast emit -12ms, ast finalize -9ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 4th - 23:02
**+4ms avg**; 0 faster, 2 slower; 16/16 cases
Avg: all ~50ms, core ~94ms, docs ~262ms, stress ~34ms, module ~21ms, borrow ~20ms
Stage movement: ast +203ms, ast emit +120ms, ast env +77ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 12th - 10:30
mixed: avg -8ms; 6 faster, 4 slower; 16/16 cases
Avg: all ~41ms, core ~56ms, docs ~221ms, stress ~32ms, module ~20ms, borrow ~20ms
Stage movement: ast -246ms, ast emit -121ms, ast env -74ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 14th - 09:30
no measurable change: avg 0ms; 16/16 cases
Avg: all ~43ms, core ~57ms, docs ~232ms, stress ~33ms, module ~20ms, borrow ~20ms
Stage movement: ast -33ms, ast env -33ms, file prep +16ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 15th - 19:28
mixed: avg -3ms; 2 faster, 2 slower; 16/16 cases
Avg: all ~39ms, core ~7ms, docs ~256ms, stress ~35ms, module ~21ms, borrow ~21ms
Stage movement: ast +201ms, ast env +121ms, ast emit +67ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 00:51
no measurable change: avg -1ms; 16/16 cases
Avg: all ~38ms, core ~8ms, docs ~244ms, stress ~35ms, module ~21ms, borrow ~21ms
Stage movement: ast +145ms, ast env +102ms, ast emit +28ms

# Frontend phases / macOS Apple Silicon (6D851D): June 18th - 01:21
**+50ms avg**; 0 faster, 8 slower; 8/8 cases
Avg: all ~124ms, core ~134ms, docs ~423ms, stress ~119ms, module ~51ms, borrow ~45ms
Stage movement: ast +263ms, ast env +188ms, ast emit +40ms

# Frontend phases / macOS Apple Silicon (6D851D): June 18th - 01:22
no measurable change: avg 0ms; 8/8 cases
Avg: all ~126ms, core ~134ms, docs ~437ms, stress ~119ms, module ~50ms, borrow ~45ms
Stage movement: ast +1ms, ast env +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 01:22
mixed: avg 0ms; 1 faster, 4 slower; 16/16 cases
Avg: all ~38ms, core ~7ms, docs ~197ms, stress ~39ms, module ~25ms, borrow ~23ms
Stage movement: ast -345ms, ast env -180ms, ast emit -138ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 01:23
no measurable change: avg 0ms; 16/16 cases
Avg: all ~38ms, core ~7ms, docs ~198ms, stress ~39ms, module ~25ms, borrow ~23ms
Stage movement: ast +31ms, ast emit +22ms, ast finalize +9ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 10:08
**0ms avg**; 0 faster, 1 slower; 16/16 cases
Avg: all ~38ms, core ~7ms, docs ~190ms, stress ~39ms, module ~25ms, borrow ~23ms
Stage movement: ast -26ms, ast emit -15ms, ast finalize -8ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 10:08
no measurable change: avg -1ms; 16/16 cases
Avg: all ~38ms, core ~7ms, docs ~185ms, stress ~39ms, module ~25ms, borrow ~22ms
Stage movement: file prep -5ms, ast +3ms, ast finalize +2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 18th - 20:08
case set changed: avg -46ms on 8/16 shared cases; 0 slower, 8 faster
Avg: all ~55ms, core ~63ms, docs ~334ms, stress ~39ms, module ~27ms, borrow ~28ms
Stage movement: ast -297ms, ast env -199ms, ast emit -63ms

# Frontend phases / macOS Apple Silicon (6D851D): June 18th - 21:53
no measurable change: avg -1ms; 16/16 cases
Avg: all ~55ms, core ~64ms, docs ~325ms, stress ~39ms, module ~27ms, borrow ~28ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 20:09
case set changed: avg -14ms on 16/25 shared cases; 2 slower, 12 faster
Avg: all ~19ms, core ~10ms, docs ~146ms, stress ~16ms, module ~12ms, borrow ~14ms
Stage movement: ast -724ms, ast env -552ms, ast emit -166ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 18th - 21:54
no measurable change: avg 0ms; 25/25 cases
Avg: all ~20ms, core ~10ms, docs ~143ms, stress ~17ms, module ~12ms, borrow ~15ms
Stage movement: ast +7ms, file prep +6ms, ast finalize +3ms

