# Busy Beaver Blaze

**A Turing machine interpreter (in Python and Rust) and space-time visualizer (in Rust compiled to native & WebAssembly).**

- [Run the visualizer in your web browser](https://carlkcarlk.github.io/busy_beaver_blaze/).
- Watch [an animation](https://www.youtube.com/watch?v=qYi5_mNLppY) made with this program.

## Articles

- Python: [How to Optimize your Python Program for Slowness: Write a Short Program That Finishes After the Universe Dies](https://towardsdatascience.com/how-to-optimize-your-python-program-for-slowness/) in *Towards Data Science*.
- Rust: [How to Optimize your Rust Program for Slowness: Write a Short Program That Finishes After the Universe Dies](https://medium.com/@carlmkadie/how-to-optimize-your-rust-program-for-slowness-eb2c1a64d184) on *Medium*.

## Features

- Python notebooks for [Turing machines](notebooks/turing_machines.ipynb) and implementing [fast-growing functions](notebooks/tetration.ipynb).
- Run the champion [Busy Beaver](https://en.wikipedia.org/wiki/Busy_beaver) Turing machines for millions of steps in less than a second.
- Simulate your own Turing machines.
- Visualize space-time diagrams as the Turing machine runs.
- Control speed, step count, and sampling vs. binning:
  - Millions of steps in less than a second.
  - A billion steps in about 5 seconds.
  - 50 billion steps in about 10 minutes.
- Supports common Turing machine formats: "Symbol Major," "State Major," and ["Standard Format"](https://discuss.bbchallenge.org/t/standard-tm-text-format/60).
- Optional **perfect binning** for tape visualization. By default, ever pixel in the image is the average of the (sometimes billions of) tape values that it represents.
- You can set settings via the URL hash fragment, for example, `#program=bb5&earlyStop=false`. Include `run=true` to run the program immediately.

## Techniques

- The Turing machine interpreter is a straightforward Rust implementation.
- The space-time visualizer uses adaptive sampling:
  - Initially records the full tape at each step.
  - If the tape or step count exceeds twice the image size, it halves the sampling rate.
  - Memory and time scale with image size, not step count or tape width.
- Even with pixel binning, memory use is a function
of the image size, not of the Turing run.
- Uses SIMD, even in WebAssembly, to speed up the pixel binning.
- For movie creation, multithreads rendering.
- Tips on porting Rust to WASM: [Nine Rules for Running Rust in the Browser](https://medium.com/towards-data-science/nine-rules-for-running-rust-in-the-browser-8228353649d1) in *Towards Data Science*.
- Tips on SIMD in Rust: [Nine Rules for SIMD Acceleration of Your Rust Code](https://www.youtube.com/watch?v=IBcJ2vRHGAY) in *Towards Data Science*.

## Web App Screenshot

![Busy Beaver Space-Time Diagram](Screenshot.png)

A space-time diagram of the best-known 6-state Busy Beaver after 10 billion steps. Each vertical slice represents the tape at a moment in time—dark pixels are 1s, light pixels are 0s. Time flows downward.

## Video

- [Turing Machine "BB6 Contender": 1 Trillion Steps with Exponential Acceleration (YouTube)](https://www.youtube.com/watch?v=IBcJ2vRHGAY)
- To render a video, edit `examples/movie.rs` to set the output directory, font, etc., then run:

  ```bash
  cargo run --example movie --release
  ```

## Related Work

- [The Busy Beaver Challenge](https://bbchallenge.org)
- [Quanta Magazine article](https://www.quantamagazine.org/amateur-mathematicians-find-fifth-busy-beaver-turing-machine-20240702/) and ["Up and Atom" video](https://www.youtube.com/watch?v=pQWFSj1CXeg&t=977s) on recent progress.
- [Fiery’s full-featured visualizer](https://fiery.pages.dev/turing/1RB1LC_0RD0RB_1RA0LC_1LD1RA) (TypeScript), used in the [Busy Beaver Challenge](https://bbchallenge.org/). It processes up to ~4 billion steps but does not animate the diagram’s development.
- For running Turing machines beyond trillions of steps, see this [math.stackexchange.com thread](https://math.stackexchange.com/questions/1202334/how-was-the-busy-beaver-candidate-fReador-6-states-calculated).

## License

This project is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)

## Contributing

Contributions are welcome! Feature suggestions and bug reports are appreciated.
