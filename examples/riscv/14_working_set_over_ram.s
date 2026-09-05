# 14_working_set_over_ram.s
# Requires: base RV32I only
# Sums an array too big to fit in physical RAM at once. This is the
# only test in this set that needs virtual memory turned on: with it
# on and RAM's frame pool smaller than N * 4 bytes, some of these
# accesses cannot be satisfied by any resident frame and force a real
# page fault, pulling a page in from disk. Without this test, nothing
# in this set ever touches the TLB or disk sections of the stats.
#
# N is the constant worth editing, together with the number of frames
# configured for RAM: pick N so that N * 4 is bigger than
# frames * page size, and turn virtual memory on before running. The
# access pattern itself is the same tight sequential scan as
# 05_sequential_scan.s, on purpose, so the only new variable here is
# whether the working set fits in RAM at all.

.data
array_base: .word 0

.text
.globl main
main:
    li      t2, 16384       # N: how many words the array holds, edit this
    la      t0, array_base
    slli    t3, t2, 2
    add     t4, t0, t3        # one-past-the-end address
    li      s0, 0             # running sum

scan:
    lw      t1, 0(t0)
    add     s0, s0, t1
    addi    t0, t0, 4
    bne     t0, t4, scan

    mv      a0, s0
    ret
