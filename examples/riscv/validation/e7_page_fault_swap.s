.data
val: .word 0
.text
start:
  li x1, 0x10000000
  li x2, 0x10001000
  addi x3, x0, 7
  sw x3, 0(x1)
  lw x4, 0(x2)
  lw x4, 0(x1)
  lw x4, 0(x2)
