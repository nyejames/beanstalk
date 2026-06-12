# June 2026 Summary

## End-to-end CLI / macOS Apple Silicon (6D851D)
Change since initial benchmark: mixed: avg -4ms; 5 faster, 7 slower; 16/16 cases
Initial: all ~46ms, core ~89ms, docs ~224ms, stress ~32ms, module ~21ms, borrow ~19ms
Latest: all ~42ms, core ~57ms, docs ~227ms, stress ~33ms, module ~20ms, borrow ~21ms
Case spread latest: ~51ms

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

# End-to-end CLI / macOS Apple Silicon (6D851D): June 13th - 12:40
no measurable change: avg +1ms; 16/16 cases
Avg: all ~42ms, core ~57ms, docs ~227ms, stress ~33ms, module ~20ms, borrow ~21ms
Stage movement: ast +54ms, ast env +34ms, ast emit +23ms

