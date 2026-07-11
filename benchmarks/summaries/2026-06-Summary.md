# June 2026 Summary

## End-to-end CLI / macOS Apple Silicon (6D851D)
Change since initial benchmark: case set changed: avg -18ms on 16/26 shared cases; 1 slower, 15 faster
Initial: all ~46ms, core ~89ms, docs ~224ms, stress ~32ms, module ~21ms, borrow ~19ms
Latest: all ~22ms, core ~5ms, docs ~271ms, stress ~14ms, module ~13ms, borrow ~9ms
Case spread latest: ~50ms

## Frontend phases / macOS Apple Silicon (6D851D)
Change since initial benchmark: case set changed: avg +126ms on 8/16 shared cases; 1 slower, 7 faster
Initial: all ~124ms, core ~134ms, docs ~423ms, stress ~119ms, module ~51ms, borrow ~45ms
Latest: all ~139ms, core ~56ms, docs ~1719ms, stress ~35ms, module ~25ms, borrow ~27ms
Case spread latest: ~408ms
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

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 03:08
no measurable change: avg 0ms; 25/25 cases
Avg: all ~20ms, core ~11ms, docs ~144ms, stress ~17ms, module ~12ms, borrow ~16ms
Stage movement: ast +14ms, file prep -6ms, hir +4ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 03:13
**+6ms avg**; 0 faster, 5 slower; 16/16 cases
Avg: all ~60ms, core ~77ms, docs ~351ms, stress ~42ms, module ~29ms, borrow ~41ms
Stage movement: borrow +2ms, ast +2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 03:14
no measurable change: avg 0ms; 16/16 cases
Avg: all ~60ms, core ~77ms, docs ~346ms, stress ~42ms, module ~29ms, borrow ~41ms
Stage movement: ast +3ms, ast env +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 03:14
**+6ms avg**; 0 faster, 11 slower; 25/25 cases
Avg: all ~26ms, core ~23ms, docs ~167ms, stress ~23ms, module ~14ms, borrow ~27ms
Stage movement: ast +9ms, ast finalize +4ms, file prep -4ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 03:15
no measurable change: avg -1ms; 25/25 cases
Avg: all ~25ms, core ~22ms, docs ~163ms, stress ~22ms, module ~13ms, borrow ~29ms
Stage movement: ast -16ms, file prep +12ms, ast emit -10ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 09:47
**+3ms avg**; 0 faster, 2 slower; 16/16 cases
Avg: all ~63ms, core ~81ms, docs ~379ms, stress ~44ms, module ~30ms, borrow ~41ms
Stage movement: ast +8ms, ast emit +4ms, file prep +4ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 09:47
**+1ms avg**; 0 faster, 2 slower; 16/16 cases
Avg: all ~64ms, core ~81ms, docs ~382ms, stress ~44ms, module ~30ms, borrow ~44ms
Stage movement: ast env +2ms, ast +1ms, ast emit -1ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 09:48
**-3ms avg**; 1 faster, 0 slower; 16/16 cases
Avg: all ~61ms, core ~79ms, docs ~345ms, stress ~43ms, module ~29ms, borrow ~43ms
Stage movement: ast -7ms, ast env -3ms, file prep -3ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 09:48
no measurable change: avg +2ms; 16/16 cases
Avg: all ~62ms, core ~79ms, docs ~347ms, stress ~45ms, module ~30ms, borrow ~44ms
Stage movement: ast +8ms, ast env +6ms, borrow +2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 09:48
**+3ms avg**; 0 faster, 2 slower; 16/16 cases
Avg: all ~66ms, core ~82ms, docs ~409ms, stress ~44ms, module ~30ms, borrow ~44ms
Stage movement: ast emit +4ms, ast +3ms, file prep +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 09:48
**+3ms avg**; 0 faster, 5 slower; 25/25 cases
Avg: all ~29ms, core ~28ms, docs ~193ms, stress ~25ms, module ~15ms, borrow ~31ms
Stage movement: ast +159ms, ast emit +92ms, ast env +45ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 09:49
no measurable change: avg -1ms; 25/25 cases
Avg: all ~27ms, core ~26ms, docs ~171ms, stress ~24ms, module ~14ms, borrow ~30ms
Stage movement: ast -78ms, ast emit -50ms, ast env -23ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 09:49
**+2ms avg**; 0 faster, 1 slower; 25/25 cases
Avg: all ~30ms, core ~26ms, docs ~196ms, stress ~27ms, module ~15ms, borrow ~30ms
Stage movement: ast +91ms, ast emit +61ms, file prep +37ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 10:19
**-6ms avg**; 7 faster, 0 slower; 16/16 cases
Avg: all ~60ms, core ~76ms, docs ~354ms, stress ~42ms, module ~29ms, borrow ~40ms
Stage movement: ast -18ms, ast emit -8ms, ast env -7ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 10:20
no measurable change: avg +1ms; 16/16 cases
Avg: all ~60ms, core ~76ms, docs ~355ms, stress ~42ms, module ~28ms, borrow ~40ms
Stage movement: ast -1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 10:20
**-5ms avg**; 10 faster, 0 slower; 25/25 cases
Avg: all ~25ms, core ~22ms, docs ~157ms, stress ~22ms, module ~13ms, borrow ~27ms
Stage movement: ast -137ms, ast emit -92ms, file prep -45ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 19th - 10:21
no measurable change: avg 0ms; 25/25 cases
Avg: all ~25ms, core ~22ms, docs ~163ms, stress ~22ms, module ~13ms, borrow ~27ms
Stage movement: ast emit -1ms, ast finalize +1ms, sort +1ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:09
mixed: avg +1ms; 2 faster, 9 slower; 16/16 cases
Avg: all ~61ms, core ~72ms, docs ~362ms, stress ~43ms, module ~31ms, borrow ~33ms
Stage movement: ast +26ms, ast env +11ms, ast emit +9ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:09
no measurable change: avg -1ms; 16/16 cases
Avg: all ~60ms, core ~69ms, docs ~362ms, stress ~42ms, module ~30ms, borrow ~32ms
Stage movement: ast -8ms, ast env -4ms, borrow -3ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:09
**+2ms avg**; 0 faster, 2 slower; 16/16 cases
Avg: all ~62ms, core ~69ms, docs ~386ms, stress ~43ms, module ~29ms, borrow ~32ms
Stage movement: ast +6ms, ast emit +3ms, borrow +2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:09
**-2ms avg**; 1 faster, 0 slower; 16/16 cases
Avg: all ~60ms, core ~70ms, docs ~367ms, stress ~42ms, module ~29ms, borrow ~32ms
Stage movement: ast -6ms, ast emit -3ms, borrow -2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:09
**0ms avg**; 0 faster, 1 slower; 16/16 cases
Avg: all ~60ms, core ~69ms, docs ~350ms, stress ~43ms, module ~31ms, borrow ~34ms
Stage movement: ast +6ms, ast emit +2ms, borrow +2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:41
**-3ms avg**; 3 faster, 0 slower; 16/16 cases
Avg: all ~58ms, core ~68ms, docs ~333ms, stress ~41ms, module ~29ms, borrow ~32ms
Stage movement: ast -12ms, ast emit -5ms, ast env -4ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 16:42
no measurable change: avg 0ms; 16/16 cases
Avg: all ~58ms, core ~68ms, docs ~335ms, stress ~41ms, module ~29ms, borrow ~31ms
Stage movement: borrow -1ms

# Frontend phases / macOS Apple Silicon (6D851D): June 19th - 17:03
**-4ms avg**; 6 faster, 0 slower; 16/16 cases
Avg: all ~54ms, core ~65ms, docs ~307ms, stress ~39ms, module ~27ms, borrow ~29ms
Stage movement: ast -13ms, ast emit -6ms, borrow -5ms

# Frontend phases / macOS Apple Silicon (6D851D): June 20th - 11:42
no measurable change: avg 0ms; 16/16 cases
Avg: all ~53ms, core ~65ms, docs ~301ms, stress ~39ms, module ~28ms, borrow ~29ms
Stage movement: file prep +2ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 20th - 10:43
**-5ms avg**; 10 faster, 0 slower; 25/25 cases
Avg: all ~20ms, core ~11ms, docs ~141ms, stress ~17ms, module ~12ms, borrow ~16ms
Stage movement: ast emit -15ms, ast env +8ms, file prep +7ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 20th - 11:43
no measurable change: avg 0ms; 25/25 cases
Avg: all ~20ms, core ~11ms, docs ~139ms, stress ~17ms, module ~12ms, borrow ~16ms
Stage movement: ast +10ms, ast finalize +4ms, ast env +3ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 10:45
**+10ms avg**; 0 faster, 5 slower; 16/16 cases
Avg: all ~63ms, core ~89ms, docs ~360ms, stress ~44ms, module ~31ms, borrow ~42ms
Stage movement: ast +20ms, ast env +12ms, ast emit +5ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 10:45
no measurable change: avg 0ms; 16/16 cases
Avg: all ~63ms, core ~79ms, docs ~367ms, stress ~45ms, module ~30ms, borrow ~43ms
Stage movement: ast env -7ms, ast -6ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 10:45
**-3ms avg**; 1 faster, 0 slower; 16/16 cases
Avg: all ~61ms, core ~80ms, docs ~339ms, stress ~44ms, module ~29ms, borrow ~42ms
Stage movement: ast -5ms, file prep -2ms, ast emit -2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 10:45
**+2ms avg**; 0 faster, 1 slower; 16/16 cases
Avg: all ~62ms, core ~78ms, docs ~365ms, stress ~43ms, module ~31ms, borrow ~43ms
Stage movement: ast +3ms, file prep +2ms, ast emit +2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 10:46
mixed: avg -1ms; 1 faster, 1 slower; 16/16 cases
Avg: all ~62ms, core ~79ms, docs ~341ms, stress ~45ms, module ~30ms, borrow ~42ms
Stage movement: ast env +2ms, ast +2ms, borrow +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 10:46
**+6ms avg**; 0 faster, 11 slower; 25/25 cases
Avg: all ~26ms, core ~23ms, docs ~158ms, stress ~23ms, module ~14ms, borrow ~29ms
Stage movement: ast +13ms, ast emit +9ms, ast env +5ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 10:46
no measurable change: avg 0ms; 25/25 cases
Avg: all ~26ms, core ~24ms, docs ~153ms, stress ~23ms, module ~14ms, borrow ~28ms
Stage movement: ast -11ms, file prep -5ms, ast finalize -5ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 10:46
**+1ms avg**; 0 faster, 1 slower; 25/25 cases
Avg: all ~27ms, core ~26ms, docs ~161ms, stress ~25ms, module ~14ms, borrow ~28ms
Stage movement: ast +35ms, ast emit +19ms, ast env +10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 10:47
no measurable change: avg +4ms; 25/25 cases
Avg: all ~31ms, core ~27ms, docs ~248ms, stress ~25ms, module ~14ms, borrow ~31ms
Stage movement: ast +268ms, ast emit +156ms, ast env +82ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 12:04
mixed: avg -1ms; 1 faster, 1 slower; 16/16 cases
Avg: all ~61ms, core ~78ms, docs ~333ms, stress ~44ms, module ~30ms, borrow ~44ms
Stage movement: ast -5ms, ast env -2ms, ast emit -2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 12:05
no measurable change: avg +1ms; 16/16 cases
Avg: all ~60ms, core ~79ms, docs ~333ms, stress ~43ms, module ~30ms, borrow ~41ms
Stage movement: ast +3ms, hir +2ms, ast env +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 12:05
**-5ms avg**; 2 faster, 0 slower; 25/25 cases
Avg: all ~26ms, core ~25ms, docs ~163ms, stress ~23ms, module ~14ms, borrow ~27ms
Stage movement: ast -281ms, ast emit -167ms, ast env -85ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 12:06
no measurable change: avg 0ms; 25/25 cases
Avg: all ~26ms, core ~23ms, docs ~155ms, stress ~24ms, module ~14ms, borrow ~27ms
Stage movement: ast -16ms, ast emit -12ms, ast env -4ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 12:06
**0ms avg**; 2 faster, 0 slower; 25/25 cases
Avg: all ~25ms, core ~23ms, docs ~155ms, stress ~23ms, module ~14ms, borrow ~29ms
Stage movement: ast emit -5ms, ast -4ms, file prep -4ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 20:55
mixed: avg +113ms; 4 faster, 2 slower; 16/16 cases
Avg: all ~173ms, core ~67ms, docs ~2188ms, stress ~41ms, module ~29ms, borrow ~31ms
Stage movement: ast +187ms, ast emit +171ms, ast env +9ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 20:55
**-2ms avg**; 1 faster, 0 slower; 16/16 cases
Avg: all ~172ms, core ~67ms, docs ~2179ms, stress ~40ms, module ~28ms, borrow ~29ms
Stage movement: ast -4ms, file prep -4ms, ast env -2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 20:56
no measurable change: avg -4ms; 16/16 cases
Avg: all ~167ms, core ~65ms, docs ~2107ms, stress ~40ms, module ~28ms, borrow ~31ms
Stage movement: ast -4ms, ast emit -3ms, file prep +3ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 20:57
**+9ms avg**; 0 faster, 1 slower; 16/16 cases
Avg: all ~176ms, core ~70ms, docs ~2250ms, stress ~40ms, module ~28ms, borrow ~29ms
Stage movement: ast +16ms, ast emit +12ms, file prep -2ms

# Frontend phases / macOS Apple Silicon (6D851D): June 21st - 20:57
no measurable change: avg -8ms; 16/16 cases
Avg: all ~168ms, core ~66ms, docs ~2119ms, stress ~41ms, module ~28ms, borrow ~29ms
Stage movement: ast -15ms, ast emit -13ms, file prep +2ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 20:58
mixed: avg +39ms; 8 faster, 1 slower; 25/25 cases
Avg: all ~65ms, core ~11ms, docs ~1237ms, stress ~18ms, module ~13ms, borrow ~19ms
Stage movement: ast +4072ms, ast emit +4012ms, ast env +31ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 20:58
no measurable change: avg -1ms; 25/25 cases
Avg: all ~64ms, core ~11ms, docs ~1217ms, stress ~18ms, module ~13ms, borrow ~17ms
Stage movement: ast emit +36ms, ast +15ms, ast env -10ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 20:59
**0ms avg**; 1 faster, 0 slower; 25/25 cases
Avg: all ~63ms, core ~12ms, docs ~1214ms, stress ~18ms, module ~13ms, borrow ~16ms
Stage movement: file prep +143ms, ast -93ms, ast emit -89ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 20:59
**+4ms avg**; 0 faster, 1 slower; 25/25 cases
Avg: all ~67ms, core ~11ms, docs ~1273ms, stress ~18ms, module ~15ms, borrow ~20ms
Stage movement: ast +200ms, ast emit +181ms, file prep -87ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 21st - 20:59
mixed: avg -1ms; 1 faster, 1 slower; 25/25 cases
Avg: all ~66ms, core ~13ms, docs ~1258ms, stress ~18ms, module ~13ms, borrow ~17ms
Stage movement: file prep -53ms, ast +6ms, ast env +5ms

# Frontend phases / macOS Apple Silicon (6D851D): June 23rd - 19:09
**-30ms avg**; 13 faster, 0 slower; 16/16 cases
Avg: all ~139ms, core ~56ms, docs ~1719ms, stress ~35ms, module ~25ms, borrow ~28ms
Stage movement: ast -102ms, ast emit -68ms, ast env -27ms

# Frontend phases / macOS Apple Silicon (6D851D): June 23rd - 19:12
no measurable change: avg -1ms; 16/16 cases
Avg: all ~139ms, core ~56ms, docs ~1719ms, stress ~35ms, module ~25ms, borrow ~27ms
Stage movement: file prep -2ms, ast emit +1ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 23rd - 19:13
**-22ms avg**; 21 faster, 0 slower; 25/25 cases
Avg: all ~44ms, core ~11ms, docs ~813ms, stress ~13ms, module ~10ms, borrow ~14ms
Stage movement: ast -2123ms, ast emit -1998ms, ast env -98ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 23rd - 19:15
no measurable change: avg 0ms; 25/25 cases
Avg: all ~44ms, core ~11ms, docs ~812ms, stress ~13ms, module ~10ms, borrow ~14ms
Stage movement: file prep +74ms, ast emit -3ms, ast env -3ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 28th - 07:30
mixed: avg -17ms; 1 faster, 17 slower; 25/25 cases
Avg: all ~26ms, core ~11ms, docs ~284ms, stress ~18ms, module ~13ms, borrow ~17ms
Stage movement: ast emit -1979ms, ast -1605ms, hir +149ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 28th - 19:26
no measurable change: avg -1ms; 25/25 cases
Avg: all ~26ms, core ~12ms, docs ~272ms, stress ~17ms, module ~13ms, borrow ~17ms
Stage movement: hir +18ms, file prep -13ms, ast env -7ms

# End-to-end CLI / macOS Apple Silicon (6D851D): June 30th - 01:19
case set changed: avg -3ms on 25/26 shared cases; 0 slower, 9 faster
Avg: all ~22ms, core ~5ms, docs ~271ms, stress ~14ms, module ~13ms, borrow ~9ms
Stage movement: ast -34ms, hir -21ms, ast finalize -12ms

