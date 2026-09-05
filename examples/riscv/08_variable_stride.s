# 08_variable_stride.s
# Requires: base RV32I only
# Walks a large array, but instead of visiting every word it jumps
# STRIDE_BYTES ahead each time. The natural companion to
# 07_pointer_chase.s: that one discovers how big each level is, this
# one discovers how big a single line is. Sweep STRIDE_BYTES in powers
# of two (4, 8, 16, 32, 64, ...) and edit it, rerunning each time: the
# miss rate stays flat while STRIDE_BYTES is smaller than a level's
# line size, since each fetched line still covers the next few
# accesses, then jumps once STRIDE_BYTES reaches the line size, since
# every access now lands in a fresh line.
#
# ARRAY_BYTES is kept fixed and large, so the total footprint stays
# well above any level's capacity no matter what STRIDE_BYTES is set
# to. That keeps this test about line size only, not about capacity,
# which is what 07_pointer_chase.s already covers. Make sure RAM is
# configured with enough room for ARRAY_BYTES before running this.

.data
array_base: .word 0

.text
.globl main
main:
    li      t0, 8192        # ARRAY_BYTES: total footprint to cover, keep this large
    li      t1, 4           # STRIDE_BYTES: edit this, sweep 4, 8, 16, 32, 64, 128...
    la      t2, array_base
    li      t3, 0           # byte offset from array_base

loop:
    bge     t3, t0, done
    add     t4, t2, t3
    lw      t5, 0(t4)
    add     t3, t3, t1
    j       loop

done:
    ret
