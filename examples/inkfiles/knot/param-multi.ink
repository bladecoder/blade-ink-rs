VAR x = 1
        VAR y = "Hmm."
        How much do you give?
        * [I don't know] -> give(x, 2, y)
        
        === give(a, b, c) ===
            You give {a} or {b} dollars. {y}
            -> END