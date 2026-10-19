# 01_base_integer.s
# Requires: base RV32I only
# Base RV32I: arithmetic, immediates, branches, a loop, and load/store
# through the memory hierarchy. Sums the integers 1..10 into a0, then
# stores the result to a stack slot and reads it back.

.data
result: .word 0

.text
.globl main
main:
    li      t0, 0           # accumulator
    li      t1, 1           # loop counter i
    li      t2, 11          # loop bound (exclusive)

loop:
    beq     t1, t2, done
    add     t0, t0, t1
    addi    t1, t1, 1
    j       loop

done:
    la      t3, result
    sw      t0, 0(t3)       # store the sum through the data hierarchy
    lw      a0, 0(t3)       # read it back

    addi    sp, sp, -16
    sw      a0, 0(sp)
    lw      a1, 0(sp)
    addi    sp, sp, 16

    slti    t4, a0, 100
    sltiu   t5, a0, 100
    and     t6, a0, a1
    or      t6, t6, a1
    xor     t6, t6, a1
    sll     t6, a1, t1
    srl     t6, a1, t1
    sra     t6, a1, t1

    ret
