.data
vals: .word 0, 0
.text
start:
  la x1, vals
  addi x2, x1, 4
  addi x3, x0, 7
  addi x4, x0, 10
loop:
  sw x3, 0(x1)
  sw x3, 0(x2)
  addi x4, x4, -1
  bne x4, x0, loop
