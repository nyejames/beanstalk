# Standard Math Library plan
This will be the implementation of the core standard math library.
The goal is to use this as a way to harden and fix bugs with the external import system.

The template for the library: 

`
@std/math

Constants:
  PI  Float
  TAU Float
  E   Float

Functions:
  sin(x Float) -> Float
  cos(x Float) -> Float
  tan(x Float) -> Float
  atan2(y Float, x Float) -> Float

  log(x Float) -> Float
  log2(x Float) -> Float
  log10(x Float) -> Float
  exp(x Float) -> Float
  pow(base Float, exponent Float) -> Float

  sqrt(x Float) -> Float
  abs(x Float) -> Float
  floor(x Float) -> Float
  ceil(x Float) -> Float
  round(x Float) -> Float
  trunc(x Float) -> Float

  min(a Float, b Float) -> Float
  max(a Float, b Float) -> Float
  clamp(x Float, min Float, max Float) -> Float
`

This won't be testing methods or Types yet, just:
- constants
- free functions
- Float ABI
- package-scoped imports
- backend lowering

for now.