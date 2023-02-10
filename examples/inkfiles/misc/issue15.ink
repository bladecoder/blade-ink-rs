// Issue reported here: https://github.com/bladecoder/blade-ink/issues/15
// The correct output has to be:
// This is a test
// X is set

VAR x = ""

This is a test
SET_X:
{
  - x == "":
    -> x_not_set
  - else:
    -> x_is_set
}
-> END

= x_not_set
X is not set!
-> END

= x_is_set
X is set
-> END