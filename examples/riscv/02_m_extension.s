# 02_m_extension.s
# Requires: M extension
# M extension: multiplication, division, and remainder, including the
# high-multiply variants and signed/unsigned division edge cases.

.text
.globl main
main:
    li      a0, 6
    li      a1, 7
    mul     t0, a0, a1      # t0 = 42

    li      a2, -3
    mul     t1, a0, a2      # t1 = -18

    li      a3, 1000000
    mul     t2, a3, a3
    mulh    t3, a3, a3      # high bits of the signed 64-bit product
    mulhu   t4, a3, a3
    mulhsu  t5, a3, a2

    li      a4, 17
    li      a5, 5
    div     s0, a4, a5      # 3
    rem     s1, a4, a5      # 2
    divu    s2, a4, a5
    remu    s3, a4, a5

    li      a6, -17
    div     s4, a6, a5      # signed division truncates toward zero
    rem     s5, a6, a5

    li      a7, 0
    div     s6, a4, a7      # division by zero: all bits set, per RISC-V spec
    rem     s7, a4, a7      # division by zero: dividend returned unchanged

    ret
