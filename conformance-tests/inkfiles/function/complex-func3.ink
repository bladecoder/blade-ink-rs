
     ~ merchant_init()
      "I will pay you {fee} reales if you get the goods to their destination. The goods will take up {weight} cargo spaces."
     -> END
     
     === function merchant_init()
     VAR weight = 20
     VAR roll = 0
     VAR mult = 1
     
     { roll == 0:
        ~ mult = 2
     }
     
     { mult == 2:
        ~ roll = 1
     }
     
     { roll == 0:
        ~ mult = 3
     }
     
     VAR dst = 5
     VAR deadline = 0
     ~ deadline = (dst * (100)) / 100
     VAR fee = 0
     ~ fee = (1 + dst) * 10 * mult
