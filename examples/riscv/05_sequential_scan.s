# 05_sequential_scan.s
# Requires: base RV32I only
# Sums an array from start to end, one word after another. This is the
# best case for spatial locality: every access lands right after the
# previous one, so once a line is fetched every other word in it is a
# hit. Use this as the baseline hit rate to compare every other test
# in this set against, especially 06_random_scan.s which sums the
# exact same kind of array in a scrambled order.
#
# N is the only thing worth editing here: the number of words in the
# array. The loop that does the actual summing is exactly four
# instructions, so its own overhead barely moves as N grows. Make sure
# RAM is configured with at least N * 4 bytes of room before running.

.data
array_base: .word 0

.text
.globl main
main:
    li      t2, 128         # N: how many words the array holds, edit this
    la      t0, array_base

    # Fill the array with array[i] = i + 1, just so the sum means something.
    li      t1, 0           # i
fill:
    beq     t1, t2, fill_done
    addi    t3, t1, 1
    sw      t3, 0(t0)
    addi    t0, t0, 4
    addi    t1, t1, 1
    j       fill
fill_done:

    # The measured loop: sum every word from start to end, one after another.
    la      t0, array_base       # cursor
    slli    t3, t2, 2
    add     t4, t0, t3           # one-past-the-end address
    li      s0, 0                # running sum

scan:
    lw      t1, 0(t0)
    add     s0, s0, t1
    addi    t0, t0, 4
    bne     t0, t4, scan

    mv      a0, s0
    ret
