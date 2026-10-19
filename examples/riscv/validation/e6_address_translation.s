.text
start:
  li x1, 0x10000000
  li x2, 0x10001000
  li x3, 0x10002000
  lw x4, 0(x1)
  lw x4, 0(x2)
  lw x4, 0(x1)
  lw x4, 0(x3)
  lw x4, 0(x1)
  lw x4, 0(x2)
