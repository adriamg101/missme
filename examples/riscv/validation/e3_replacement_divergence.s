.data
vals: .word 0, 0, 0
.text
start:
  la x1, vals
  addi x2, x1, 4
  addi x3, x1, 8
  lw x4, 0(x1)
  lw x5, 0(x2)
  lw x4, 0(x1)
  lw x4, 0(x1)
  lw x6, 0(x3)
  lw x5, 0(x2)
  lw x6, 0(x3)
  lw x6, 0(x3)
