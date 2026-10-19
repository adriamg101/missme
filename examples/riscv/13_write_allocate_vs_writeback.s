# 13_write_allocate_vs_writeback.s
# Requires: base RV32I only
# Two write-only phases in one file, since both are about config
# toggles rather than different code. Nothing here needs editing, the
# access pattern is the point: run it once per config value and read
# the stats after each run, or pause between phases and compare the
# running totals before and after phase 2.
#
# Phase 1 fills a fresh array, writing every word exactly once without
# ever reading it first. With write-allocate on, every one of those
# misses still fetches the whole line before overwriting it, wasted
# traffic for data that is about to be replaced entirely. Toggle
# write-allocate off for the level under test and rerun: that fetch
# traffic should disappear from the level below.
#
# Phase 2 increments the exact same word, in place, many times in a
# row. With write-through, every single one of those stores also goes
# straight to RAM. With write-back, the line just stays dirty in cache
# and only gets written back once, at the very end. Toggle the write
# policy for the level under test and rerun: the RAM-level write count
# should swing wildly between the two.

.data
array_base: .word 0
counter_base: .word 0

.text
.globl main
main:
    li      t2, 128         # words to fill in phase 1

    # Phase 1: write-only fill, never reads a word before overwriting it.
    la      t0, array_base
    li      t1, 0
fill:
    beq     t1, t2, fill_done
    sw      t1, 0(t0)
    addi    t0, t0, 4
    addi    t1, t1, 1
    j       fill
fill_done:

    # Phase 2: increment the exact same word, over and over.
    la      t0, counter_base
    li      t3, 2000        # how many times to increment
count:
    beqz    t3, done
    lw      t4, 0(t0)
    addi    t4, t4, 1
    sw      t4, 0(t0)
    addi    t3, t3, -1
    j       count

done:
    ret
