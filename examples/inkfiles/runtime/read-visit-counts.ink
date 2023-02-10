VAR x = 0

-> one

=== one ===
{ x < 4:
 -> two.s2
- else:
 -> two
}

=== two ===
-> END

= s2
~ x = x + 1
-> one
