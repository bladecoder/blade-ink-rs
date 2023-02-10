~ derp(2, 3, 4)
   The values are {x} and {y}.
   -> END
   
   === function derp(a, b, c) ===
   VAR x = 0
   ~ x = a + b
   VAR y = 3
   { x == 5:
      ~ x = 6
   }
   ~ y = x + c