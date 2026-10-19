.data
bufA: .word 0
.text
start:
  la x1, bufA
  addi x2, x1, 256
  addi x3, x0, 20
loop:
  lw x4, 0(x1)
  lw x5, 0(x2)
  addi x3, x3, -1
  bne x3, x0, loop
