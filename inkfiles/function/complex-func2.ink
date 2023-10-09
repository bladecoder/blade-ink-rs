~ derp(2, 3)
    The values are {x} and {y} and {z}.
    -> END
    
    === function derp(a, b) ===  
    VAR x = 0
   ~ x = a - b
    VAR y = 3
    {
      - x == 0:
        ~ y = 0
      - x > 0:
        ~ y = x - 1
      - else:
        ~ y = x + 1
    }
    VAR z = 1