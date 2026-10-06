# MISSME 😢

> **M**ISSME **I**s a **S**imulation **S**ystem for **M**emory hi**E**rarchies

A memory hierarchy simulator. Describe a cache hierarchy, optionally with virtual memory, run a program on it, and watch each access step by step. RISC-V is the first supported language; others can be added.

## Features

- Configurable cache levels: number of sets, block size, associativity, and separate or unified instruction and data caches.
- Replacement policies: LRU, FIFO, LFU, and Random.
- Write policies: write-back and write-through, with write-allocate or no-write-allocate.
- Optional virtual memory: TLB, multi-level page table, physical frames, and disk swapping.
- Step through each memory access or each instruction, or run continuously. Breakpoints can be added or removed while the program runs.
- Live views of each level: cache sets, TLB entries, page table, frames, RAM, and disk.
- Statistics per level: hits, misses, hit rate, cycles, AMAT, CPI, and MPKI.

## Download

Pre-built packages are available on the [Releases](https://github.com/adriamg101/missme/releases) page:

- Windows: `.msi` or `.exe`
- macOS: `.dmg`
- Linux: `.AppImage`, `.deb`, or `.rpm`

The packages are not signed yet, so Windows and macOS may show a security warning the first time you open them.

## Build from source

You need:

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your operating system

Then:

```bash
npm install
npm run tauri dev      # run the app in development mode
npm run tauri build    # create the installers for your platform
```

## Usage

1. Open the editor and write a program in RISC-V assembly.
2. Set the memory map for `.text` and `.data`, and the ISA extensions you want to use.
3. Configure the memory hierarchy: caches, TLB, virtual memory, RAM, and disk.
4. Start the simulation. You can step one memory access at a time, one instruction at a time, or run until a breakpoint.
5. Click the line numbers in the editor to add or remove breakpoints, even while the simulation is running.
6. Use the views to inspect what happens at each level, and the statistics panel to compare configurations.

## Program format

Programs are written in RISC-V assembly. Put code in the `.text` section and data in the `.data` section:

```asm
.data
values: .word 1, 2, 3, 4

.text
.globl main
main:
    la   t0, values
    lw   t1, 0(t0)
    ret
```

## Examples

The `examples/riscv` folder contains sample programs you can load into the editor.

| File | Description |
| --- | --- |
| `01_base_integer.s` | Base RV32I arithmetic, branches, loops, and loads and stores. |
| `02_m_extension.s` | Multiplication, division, and remainder, including edge cases. |
| `03_f_extension.s` | Single-precision float loads, stores, arithmetic, and comparisons. |
| `04_d_extension.s` | Double-precision arithmetic and conversions. |
| `05_sequential_scan.s` | Sums an array in order, the best case for spatial locality. |
| `06_random_scan.s` | Sums the same array in random order, to compare with sequential access. |
| `07_pointer_chase.s` | Follows a chain of pointers to reveal the size of each cache level. |
| `08_variable_stride.s` | Walks an array with a configurable stride. |
| `09_mapping_conflicts.s` | Addresses that map to the same cache set, causing conflict misses. |
| `10_temporal_reuse.s` | Scans the same buffer repeatedly, to show temporal locality. |
| `11_matrix_row_vs_column.s` | Sums a matrix by rows and by columns to compare access patterns. |
| `12_matrix_multiply_naive_vs_tiled.s` | Matrix multiplication with and without loop tiling. |
| `13_write_allocate_vs_writeback.s` | Compares write policies on the same store pattern. |
| `14_working_set_over_ram.s` | An array larger than physical RAM, which needs virtual memory. |

## Project structure

```
missme/
├── examples/
│   └── riscv/             Sample RISC-V programs
├── src/                   Frontend (Vue 3 + TypeScript)
│   ├── components/        UI components
│   │   ├── compile-config/     Memory map and compile settings
│   │   ├── hierarchy-config/   Cache, TLB, RAM, and disk configuration
│   │   ├── simulation/         Live views and statistics
│   │   └── ui/                 Shared controls
│   ├── lib/               Hierarchy model, virtual scrolling, and helpers
│   └── stores/            Pinia stores for configuration and simulation state
└── src-tauri/             Backend (Rust, Tauri)
    └── src/
        ├── controllers/   Tauri commands called by the frontend
        └── models/
            ├── interpreter/       Language frontends (lexer, parser, code generation)
            │   └── riscv/         RISC-V implementation
            └── simulation/        CPU simulator, session control, statistics
                └── memory/        Cache, TLB, page table, frames, RAM, disk, replacement policies
```

## Tests

```bash
cd src-tauri && cargo test    # backend tests
npm test                      # frontend tests
```

## Contributing

To report a bug or propose a feature, open an issue. For larger changes, discuss them in an issue first, then send a pull request that includes the tests for your changes.

To add a language, implement its frontend under `src-tauri/src/models/interpreter/`, following the RISC-V frontend.

## License

MISSME is licensed under the [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for the copyright notice.

## Building on MISSME

If you use MISSME as the basis for another tool, please credit the original project in your README or documentation, and keep the `NOTICE` file with your distribution as the license requires.
