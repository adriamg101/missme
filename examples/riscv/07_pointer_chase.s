# 07_pointer_chase.s
# Requires: base RV32I only
# The most useful test in this set for discovering a hierarchy's own
# sizes instead of being told them. Builds a chain of N nodes, each one
# holding the real address of the next node, arranged so following the
# chain visits every node exactly once before looping back to the
# start. Then it just follows the chain, over and over, doing nothing
# but "go to wherever this node points."
#
# This is unlike every other test in this set: each address depends on
# the value just loaded, so nothing can guess the next address ahead of
# time. The working set is exactly N * 4 bytes. Run this same file
# again and again, each time editing N to a different value, and watch
# the miss rate: it stays low while N * 4 fits in a level, then jumps
# once it doesn't. Do that sweep and the jumps mark out the real size
# of every level in the hierarchy, in order, without needing to be told
# any of them up front.
#
# The chain order is not stored as a list, it is generated on the fly
# the same way as in 06_random_scan.s: node i points at node
# (i + K) mod N. This only reaches every node if K and N share no
# common factor, so if you change either one, check that first, don't
# just keep K odd: 65 and 200 are both odd and small, but they share a
# factor of 5, so the chain only visits a fifth of the nodes and loops
# early. Make sure RAM is configured with at least N * 4 bytes of room
# before running.

.data
chain_base: .word 0

.text
.globl main
main:
    li      t2, 128         # N: how many nodes the chain holds, edit this
    li      t6, 53          # K: step used to scramble the chain order
    li      t5, 4000        # how many times to follow the chain, keep well above N

    # Build the chain: node[i] = address of node[(i + K) mod N].
    la      s2, chain_base       # chain base, kept for address math
    li      t0, 0                # i
    li      t1, 0                # (i + K) mod N, built incrementally
build:
    beq     t0, t2, build_done
    slli    t3, t0, 2
    add     t3, s2, t3           # &chain[i]
    slli    t4, t1, 2
    add     t4, s2, t4           # &chain[(i + K) mod N]
    sw      t4, 0(t3)

    add     t1, t1, t6
    blt     t1, t2, next_i
    sub     t1, t1, t2           # wrap back into range, K < N so once is enough
next_i:
    addi    t0, t0, 1
    j       build
build_done:

    # The measured loop: just follow the chain, one dependent load at a time.
    mv      a0, s2                # current node's address, starts at node 0
chase:
    beqz    t5, finish
    lw      a0, 0(a0)
    addi    t5, t5, -1
    j       chase

finish:
    ret
