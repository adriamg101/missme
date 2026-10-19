.data
array:
.word 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
.word 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
.word 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
.word 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
.text
start:
  la x1, array
  addi x2, x0, 64
loop:
  lw x3, 0(x1)
  addi x1, x1, 4
  addi x2, x2, -1
  bne x2, x0, loop
