-> add_one_to_one_hundred(0, 1)
        
        === add_one_to_one_hundred(total, x) ===
            ~ total = total + x
            { x == 15:
                -> finished(total)
            - else:
                -> add_one_to_one_hundred(total, x + 1)
            }
        
        === finished(total) ===
            "The result is {total}!" you announce.
            Gauss stares at you in horror.
            -> END