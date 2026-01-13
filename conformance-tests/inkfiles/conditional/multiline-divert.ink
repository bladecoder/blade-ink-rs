->test

=== test
        { stopping:
            -   At the table, I drew a card. Ace of Hearts.
            -   <> 2 of Diamonds.
                "Should I hit you again," the croupier asks.
            -   <> King of Spades.
                -> he_crowed
        }
        + [Draw a card] I drew a card. -> test
        
        == he_crowed
        "You lose," he crowed.
        
        -> END