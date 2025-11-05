# Busy Beaver Blaze

**A Turing machine interpreter and visualizer** in Rust, Python, and WebAssembly.

- [Run the visualizer in your web browser](https://carlkcarlk.github.io/busy_beaver_blaze/).
- Watch [an animation](https://www.youtube.com/watch?v=qYi5_mNLppY) made with this program.

## Articles & Talks

- Python: [How to Optimize your Python Program for Slowness: Write a Short Program That Finishes After the Universe Dies](https://towardsdatascience.com/how-to-optimize-your-python-program-for-slowness/) in *Towards Data Science*.
- Rust: [How to Optimize your Rust Program for Slowness: Write a Short Program That Finishes After the Universe Dies](https://medium.com/@carlmkadie/how-to-optimize-your-rust-program-for-slowness-eb2c1a64d184) on *Medium*.
- [Video] Rust: [How to Optimize Rust for Slowness: Inspired by new Turing Machine Results](https://www.youtube.com/watch?v=ec-ucXJ4x-0) for the Seattle Rust Users Group on *RustVideos*.

## Features

- Python notebooks: [Turing machine basics](notebooks/turing_machines.ipynb), [interactive visualization](notebooks/interactive.ipynb), [fast-growing functions (tetration)](notebooks/tetration.ipynb), and [counting to infinity](notebooks/rule_set_1_and_2.ipynb). (See [Python Quickstart](#python-quickstart) below.)
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

- Tips on Python integration with Rust: [*Nine Rules for Writing Python Extensions in Rust*](https://medium.com/data-science/nine-rules-for-writing-python-extensions-in-rust-d35ea3a4ec29) in *Towards Data Science*  

- Tips on porting Rust to WebAssembly: [*Nine Rules for Running Rust in the Browser*](https://medium.com/towards-data-science/nine-rules-for-running-rust-in-the-browser-8228353649d1) in *Towards Data Science*  
  
- Tips on SIMD in Rust:[*Nine Rules for SIMD Acceleration of Your Rust Code*](https://medium.com/data-science/nine-rules-for-simd-acceleration-of-your-rust-code-part-1-c16fe639ce21) in *Towards Data Science*  
  
## Web App Screenshot

![Busy Beaver Space-Time Diagram](Screenshot.png)

- [Run the visualizer in your web browser](https://carlkcarlk.github.io/busy_beaver_blaze/).

## Related Work

- [The Busy Beaver Challenge](https://bbchallenge.org)
- [Quanta Magazine article](https://www.quantamagazine.org/amateur-mathematicians-find-fifth-busy-beaver-turing-machine-20240702/) and ["Up and Atom" video](https://www.youtube.com/watch?v=pQWFSj1CXeg&t=977s) on recent progress.
- [Fiery’s full-featured visualizer](https://fiery.pages.dev/turing/1RB1LC_0RD0RB_1RA0LC_1LD1RA) (TypeScript), used in the [Busy Beaver Challenge](https://bbchallenge.org/). It processes up to ~4 billion steps but does not animate the diagram’s development.
- For running Turing machines beyond trillions of steps, see this [math.stackexchange.com thread](https://math.stackexchange.com/questions/1202334/how-was-the-busy-beaver-candidate-fReador-6-states-calculated).

## Python Quickstart

Clone the repository and use [uv](https://github.com/astral-sh/uv) to set up a Python environment and build the Rust extension before running notebooks or tests.

1. Clone the repository:

   ```bash
   git clone https://github.com/CarlKCarlK/busy_beaver_blaze.git
   cd busy_beaver_blaze
   ```

2. Create a virtual environment with [uv](https://github.com/astral-sh/uv):

   ```bash
   uv venv
   ```

3. Activate it: `source .venv/bin/activate` (macOS/Linux) or `.venv\Scripts\Activate.ps1` (PowerShell).
4. Install the Python dependencies in editable mode:

   ```bash
   uv pip install -e ".[dev]"
   ```

5. Build and install the Rust/PyO3 extension into that environment (requires [Rust](https://www.rust-lang.org/tools/install)):

   ```bash
   uv tool run maturin develop --release --features python
   ```

6. (Optional) Run the Python test suite:

   ```bash
   uv run pytest
   ```

7. Launch notebooks:

   ```bash
   cd notebooks
   uv run jupyter lab
   ```

   **Note for WSL users:** The browser may not open automatically. Copy the URL from the terminal output (e.g., `http://localhost:8888/lab?token=...`) and paste it into your Windows browser.

## License

This project is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)

## Contributing

Contributions are welcome! Feature suggestions and bug reports are appreciated.
