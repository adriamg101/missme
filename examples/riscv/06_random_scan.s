# 06_random_scan.s
# Requires: base RV32I only
# Sums the same kind of array as 05_sequential_scan.s, same N, same
# number of accesses, but visits every index exactly once in a
# scrambled order instead of left to right. This is the worst case for
# spatial locality: consecutive accesses almost never share a line, and
# once N is large enough the TLB gets punished too, since consecutive
# accesses often land on different pages as well. Compare its hit rate
# straight against 05_sequential_scan.s with the same N: together they
# mark out the full range of hit rates this hierarchy can produce.
#
# The scrambled order is not a giant list of random numbers, it is
# generated on the fly: visiting index, index + K, index + 2K, ... all
# taken modulo N, with K chosen so it shares no common factor with N.
# That visits every index exactly once, in an order that jumps all
# over the array instead of walking through it.
#
# N is the constant worth editing, same meaning as in the sequential
# test. This only reaches every index if K and N share no common
# factor, so check that if you change either one, don't just keep K
# odd: that alone is not enough once N stops being a power of two.
# Make sure RAM is configured with at least N * 4 bytes of room before
# running.

.data
array_base: .word 0

.text
.globl main
main:
    li      t2, 128         # N: how many words the array holds, edit this
    li      t6, 53          # K: step used to scramble the visiting order
    la      t0, array_base

    # Fill the array with array[i] = i + 1, in plain order, so the sum means something.
    li      t1, 0
fill:
    beq     t1, t2, fill_done
    addi    t3, t1, 1
    sw      t3, 0(t0)
    addi    t0, t0, 4
    addi    t1, t1, 1
    j       fill
fill_done:

    # The measured loop: same N accesses, visiting order scrambled by +K mod N.
    la      s2, array_base       # array base, kept for address math
    li      s1, 0                # running sum
    li      t0, 0                # current visiting index
    li      t1, 0                # count of accesses done so far

scan:
    beq     t1, t2, done
    slli    t3, t0, 2
    add     t4, s2, t3
    lw      t5, 0(t4)
    add     s1, s1, t5

    add     t0, t0, t6           # index += K
    blt     t0, t2, no_wrap
    sub     t0, t0, t2           # wrap back into range, K < N so once is enough
no_wrap:
    addi    t1, t1, 1
    j       scan

done:
    mv      a0, s1
    ret
