# Aeolus

A work-in-progress plugin that converts monophonic audio into MIDI data. The goal is to be able to control a MIDI synthesizer with one's voice, a theremin, or another monophonic instrument.

To control a synthesizer with manual gestures, see my other project [Galatea](f77a71eaac06580caa7c9a4fa394b57e89bdf641).

## Building

After installing [Rust](https://rustup.rs/), you can compile Aeolus as follows:

```shell
cargo xtask bundle aeolus --release
```
