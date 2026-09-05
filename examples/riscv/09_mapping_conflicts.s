# 09_mapping_conflicts.s
# Requires: base RV32I only
# Reads NUM_STREAMS addresses that are all exactly SET_STRIDE_BYTES
# apart, over and over. Addresses that far apart always land in the
# same cache set, so with a direct-mapped cache (associativity 1) and
# NUM_STREAMS greater than 1, every single access evicts the one
# before it: the miss rate jumps straight to 100%, even though the
# whole working set is tiny. Raise the level's associativity to at
# least NUM_STREAMS and the misses disappear, since the set can now
# hold all of them at once. This is the clearest way to see what
# associativity is actually for.
#
# SET_STRIDE_BYTES has to match the level being tested: it needs to be
# a multiple of (number of sets) * (line size) for that level, so that
# every stream really does land in the same set. Check the level's own
# config in the app to work that out. NUM_STREAMS is the other thing
# worth editing, together with the level's associativity. Make sure
# RAM has room for NUM_STREAMS * SET_STRIDE_BYTES bytes.

.data
array_base: .word 0

.text
.globl main
main:
    li      s0, 3           # NUM_STREAMS: how many addresses share a set, edit this
    li      s1, 4096        # SET_STRIDE_BYTES: must match (sets * line size) for the level under test
    li      s3, 500         # how many rounds to repeat

outer:
    beqz    s3, done
    la      t0, array_base       # running address, reset each round
    li      t1, 0                # stream counter
inner:
    beq     t1, s0, next_round
    lw      t2, 0(t0)
    add     t0, t0, s1
    addi    t1, t1, 1
    j       inner
next_round:
    addi    s3, s3, -1
    j       outer

done:
    ret
