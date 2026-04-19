=== loop_label_threading_min ===

- (loop)

* (asked) Player> Ask once.
    NPC> First answer.
    -> loop

+ {asked} Player> Ask follow-up.
    NPC> Follow-up answer.
    -> loop

+ Player> Leave.
    NPC> Bye.

-> END
