# 04_d_extension.s
# Requires: D extension (which needs F too, D only adds the double-width ops)
# D extension: double-precision arithmetic and conversion, built from
# integer immediates via fcvt.d.w since this project's .data directives
# don't support raw 64-bit float literals.

.text
.globl main
main:
    li      t0, 355
    li      t1, 113
    fcvt.d.w fa0, t0        # fa0 = 355.0
    fcvt.d.w fa1, t1        # fa1 = 113.0
    fdiv.d  fa2, fa0, fa1   # a close approximation of pi

    li      t2, 2
    fcvt.d.w fa3, t2
    fmul.d  fa4, fa2, fa3   # 2*pi

    fsub.d  fa5, fa4, fa2   # pi
    fadd.d  fa6, fa5, fa2   # 2*pi again

    flt.d   t3, fa5, fa4
    fle.d   t4, fa2, fa2
    feq.d   t5, fa5, fa2

    fcvt.w.d t6, fa2        # truncated back to int -> 3
    fcvt.d.w fa7, t6

    fsqrt.d fs0, fa4

    ret
