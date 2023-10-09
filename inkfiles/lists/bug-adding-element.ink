LIST gameState = KNOW_ALIEN_REPORT  

- (init)

+   a
    ~ gameState += KNOW_ALIEN_REPORT
    -> init

+  {gameState ? KNOW_ALIEN_REPORT} OK
    -> init

+ FAIL
    -> END

