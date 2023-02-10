
        VAR x = -2
        VAR y = 3
        {
            - x == 0:
                ~ y = 0
            - x > 0:
                ~ y = x - 1
            - else:
                ~ y = x + 1
        }
        The value is {y}. -> END