# 12_matrix_multiply_naive_vs_tiled.s
# Requires: M and D extensions
# Multiplies two N x N matrices of doubles two ways: the textbook
# triple loop, and the same triple loop split into BLOCK_SIZE x
# BLOCK_SIZE tiles. Both do the exact same arithmetic and produce the
# same result, but the naive version streams through a whole row of B
# for every single row of A, re-reading far more of B than fits in any
# small cache. The tiled version finishes one small block of the
# output completely before moving on, so the pieces of A and B it
# needs stay resident and get reused instead of evicted. Sweep
# BLOCK_SIZE (2, 4, 8, ...) and rerun: there is a real optimum, and it
# tracks the cache's own capacity, not just "bigger is always better."
#
# M does the row and column index math (i * N + j and friends), D
# holds the matrix entries themselves. Make sure RAM has room for
# 4 * N * N * 8 bytes (A, B, and two output matrices).

.data
a_base: .word 0
b_base: .word 0
c_naive_base: .word 0
c_tiled_base: .word 0

.text
.globl main
main:
    li      s0, 12          # N: matrix width and height, edit this
    li      s1, 4           # BLOCK_SIZE: edit this, must divide N evenly
    mul     s2, s0, s0      # N * N, element count of one matrix

    # Fill A[i] = float(i), B[i] = float(i + 1), as doubles.
    la      t0, a_base
    la      t1, b_base
    li      t2, 0
fill:
    beq     t2, s2, fill_done
    fcvt.d.w fa0, t2
    fsd     fa0, 0(t0)
    addi    t3, t2, 1
    fcvt.d.w fa1, t3
    fsd     fa1, 0(t1)
    addi    t0, t0, 8
    addi    t1, t1, 8
    addi    t2, t2, 1
    j       fill
fill_done:

    # Naive: C[i][j] = sum over k of A[i][k] * B[k][j].
    li      t0, 0            # i
naive_i:
    beq     t0, s0, naive_i_done
    li      t1, 0            # j
naive_j:
    beq     t1, s0, naive_j_done
    fcvt.d.w fa2, zero       # sum
    li      t2, 0            # k
naive_k:
    beq     t2, s0, naive_k_done
    mul     t3, t0, s0
    add     t3, t3, t2
    slli    t3, t3, 3
    la      t4, a_base
    add     t4, t4, t3
    fld     fa0, 0(t4)       # A[i][k]

    mul     t3, t2, s0
    add     t3, t3, t1
    slli    t3, t3, 3
    la      t4, b_base
    add     t4, t4, t3
    fld     fa1, 0(t4)       # B[k][j]

    fmul.d  fa0, fa0, fa1
    fadd.d  fa2, fa2, fa0
    addi    t2, t2, 1
    j       naive_k
naive_k_done:
    mul     t3, t0, s0
    add     t3, t3, t1
    slli    t3, t3, 3
    la      t4, c_naive_base
    add     t4, t4, t3
    fsd     fa2, 0(t4)       # C_naive[i][j]
    addi    t1, t1, 1
    j       naive_j
naive_j_done:
    addi    t0, t0, 1
    j       naive_i
naive_i_done:

    # Tiled: same triple loop, split into BLOCK_SIZE x BLOCK_SIZE tiles.
    li      t0, 0            # ii, tile row start
tiled_ii:
    beq     t0, s0, tiled_done
    li      t1, 0            # jj, tile column start
tiled_jj:
    beq     t1, s0, tiled_ii_next

    # Zero this tile's block of C_tiled before the kk loop starts
    # accumulating into it, instead of reading stale memory.
    add     t5, t0, s1
    add     t6, t1, s1
    mv      s4, t0            # i, within the tile
zero_i:
    beq     s4, t5, zero_i_done
    mv      s5, t1            # j, within the tile
zero_j:
    beq     s5, t6, zero_j_done
    mul     t3, s4, s0
    add     t3, t3, s5
    slli    t3, t3, 3
    la      t4, c_tiled_base
    add     t4, t4, t3
    fcvt.d.w fa2, zero
    fsd     fa2, 0(t4)
    addi    s5, s5, 1
    j       zero_j
zero_j_done:
    addi    s4, s4, 1
    j       zero_i
zero_i_done:

    li      t2, 0            # kk, tile depth start
tiled_kk:
    beq     t2, s0, tiled_jj_next

    add     t5, t0, s1
    add     t6, t1, s1       # row and column bounds for this tile
    add     s3, t2, s1       # depth bound for this tile

    mv      s4, t0           # i, within the tile
tiled_i:
    beq     s4, t5, tiled_i_done
    mv      s5, t1           # j, within the tile
tiled_j:
    beq     s5, t6, tiled_j_done

    mul     t3, s4, s0
    add     t3, t3, s5
    slli    t3, t3, 3
    la      t4, c_tiled_base
    add     t4, t4, t3
    fld     fa2, 0(t4)       # running C_tiled[i][j], accumulated across k tiles

    mv      s6, t2           # k, within the tile
tiled_k:
    beq     s6, s3, tiled_k_done
    mul     t3, s4, s0
    add     t3, t3, s6
    slli    t3, t3, 3
    la      t4, a_base
    add     t4, t4, t3
    fld     fa0, 0(t4)       # A[i][k]

    mul     t3, s6, s0
    add     t3, t3, s5
    slli    t3, t3, 3
    la      t4, b_base
    add     t4, t4, t3
    fld     fa1, 0(t4)       # B[k][j]

    fmul.d  fa0, fa0, fa1
    fadd.d  fa2, fa2, fa0
    addi    s6, s6, 1
    j       tiled_k
tiled_k_done:
    mul     t3, s4, s0
    add     t3, t3, s5
    slli    t3, t3, 3
    la      t4, c_tiled_base
    add     t4, t4, t3
    fsd     fa2, 0(t4)
    addi    s5, s5, 1
    j       tiled_j
tiled_j_done:
    addi    s4, s4, 1
    j       tiled_i
tiled_i_done:
    mv      t2, s3            # advance kk to the end of this depth tile
    j       tiled_kk
tiled_jj_next:
    add     t1, t1, s1
    j       tiled_jj
tiled_ii_next:
    add     t0, t0, s1
    j       tiled_ii
tiled_done:

    ret
