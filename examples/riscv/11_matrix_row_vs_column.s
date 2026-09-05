# 11_matrix_row_vs_column.s
# Requires: F extension
# Sums the same R x C matrix of floats twice, doing the exact same
# number of additions both times, only the order of the two nested
# loops is swapped. Row major (outer loop over rows, inner loop over
# columns) walks the matrix exactly the way it is stored, so it is one
# long sequential scan: excellent spatial locality. Column major
# (loops swapped) jumps a whole row's width on every single access,
# since a matrix is stored row by row: terrible spatial locality, for
# the exact same work. This is the clearest example of locality in
# ordinary looking code, and the easiest to reason about: nothing here
# is unusual except the order of two loop headers. Make sure RAM has
# room for R * C * 4 bytes.

.data
matrix_base: .word 0

.text
.globl main
main:
    li      s4, 24          # R: rows, edit this
    li      s5, 24          # C: columns, edit this
    slli    s6, s5, 2       # row stride in bytes = C * 4

    # total_bytes = R * row stride, built by repeated addition (no M extension here).
    li      s7, 0            # total_bytes
    mv      t5, s4
total_loop:
    beqz    t5, total_done
    add     s7, s7, s6
    addi    t5, t5, -1
    j       total_loop
total_done:

    # Fill the matrix with matrix[i][j] = float(linear index), in storage order.
    la      t0, matrix_base
    add     t1, t0, s7       # one-past-the-end address
    li      t2, 0            # linear index, used only for the fill value
fill:
    beq     t0, t1, fill_done
    fcvt.s.w fa0, t2
    fsw     fa0, 0(t0)
    addi    t0, t0, 4
    addi    t2, t2, 1
    j       fill
fill_done:

    # Row major: walks storage order exactly, one long sequential scan.
    la      t0, matrix_base
    add     t1, t0, s7       # one-past-the-end address
    fmv.w.x fa1, zero        # running sum
row_major:
    flw     fa0, 0(t0)
    fadd.s  fa1, fa1, fa0
    addi    t0, t0, 4
    bne     t0, t1, row_major

    # Column major: same number of additions, worst possible order.
    fmv.w.x fa2, zero        # running sum
    li      t3, 0            # j
col_outer:
    beq     t3, s5, col_done
    la      t0, matrix_base
    slli    t4, t3, 2
    add     t0, t0, t4       # &matrix[0][j]
    li      t6, 0            # i
col_inner:
    beq     t6, s4, col_next_j
    flw     fa0, 0(t0)
    fadd.s  fa2, fa2, fa0
    add     t0, t0, s6       # jump a whole row
    addi    t6, t6, 1
    j       col_inner
col_next_j:
    addi    t3, t3, 1
    j       col_outer
col_done:

    ret
