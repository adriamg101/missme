# 10_temporal_reuse.s
# Requires: base RV32I only
# Scans the same N-word buffer REPS times in a row, left to right each
# time. Every other test in this set is mostly about spatial locality,
# reusing nearby addresses. This one is about temporal locality,
# reusing the exact same addresses again and again: the first pass
# through is cold and mostly misses, but once the buffer is warm every
# later pass should mostly hit, as long as N * 4 bytes still fits in
# the level being tested. Sweep N upward, rerunning each time, and
# watch the hit rate stay high right up until N stops fitting, then
# drop sharply. That is a much sharper, more visible edge than in
# 07_pointer_chase.s, precisely because everything here gets reused
# instead of visited once.

.data
buffer_base: .word 0

.text
.globl main
main:
    li      t2, 96          # N: how many words the buffer holds, edit this
    li      t5, 8           # REPS: how many full passes to make
    la      t0, buffer_base

    # Fill the buffer once, so the sum means something.
    li      t1, 0
fill:
    beq     t1, t2, fill_done
    addi    t3, t1, 1
    sw      t3, 0(t0)
    addi    t0, t0, 4
    addi    t1, t1, 1
    j       fill
fill_done:

    li      s0, 0                # running sum across every pass
    slli    s1, t2, 2            # buffer size in bytes

outer:
    beqz    t5, done
    la      t0, buffer_base       # cursor
    add     t4, t0, s1            # one-past-the-end address

pass:
    lw      t1, 0(t0)
    add     s0, s0, t1
    addi    t0, t0, 4
    bne     t0, t4, pass

    addi    t5, t5, -1
    j       outer

done:
    mv      a0, s0
    ret
