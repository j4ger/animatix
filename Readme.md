# Animatix

Animatix aims to be a declarative animation language, which enables solo creators to quickly compose explanatory animations.

## Demo

```
// demo.actx
ball: Circle, at: (0, 0), color: blue

#0s
  fade-in ball [1s]

#2s
  ball: Circle, at: (100, 0), color: red [2s]  // Morphs automatically

#4s
  fade-out ball [1s, ease: EaseOutExpo]
```

## Quick Start

WIP

