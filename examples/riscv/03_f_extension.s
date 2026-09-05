# 03_f_extension.s
# Requires: F extension
# F extension: single-precision float load/store, arithmetic, integer
# conversion, and comparisons.

.data
fvals: .word 0x40490FDB, 0x402DF854   # pi and e, as raw f32 bit patterns

.text
.globl main
main:
    la      t0, fvals
    flw     fa0, 0(t0)      # fa0 = pi
    flw     fa1, 4(t0)      # fa1 = e

    fadd.s  fa2, fa0, fa1
    fsub.s  fa3, fa0, fa1
    fmul.s  fa4, fa0, fa1
    fdiv.s  fa5, fa0, fa1
    fsqrt.s fa6, fa0

    flt.s   t1, fa1, fa0    # e < pi -> 1
    fle.s   t2, fa0, fa0    # pi <= pi -> 1
    feq.s   t3, fa0, fa1    # pi == e -> 0

    fcvt.w.s  t4, fa0       # pi truncated to int -> 3
    fcvt.s.w  fa7, t4       # back to float -> 3.0

    li      t5, -2000000000
    fcvt.s.w fs0, t5        # large negative int to float

    fsw     fa2, 0(t0)      # store the sum back over pi's slot

    ret
