LIST list1 = (a1), b1, c1
LIST list2 = a2, b2, c2
LIST list3 = a3, b3, c3
VAR vlist = ()

{LIST_ALL(list1)}
{list1}

~list2 += a1
~list2 += b2

{list2}
count:{LIST_COUNT(list2)}

~list2 += c2

max:{LIST_MAX(list2)}
min:{LIST_MIN(list2)}

// Equality
~temp t = list2
{t == list2}
{t == (a1, b2, c2)}
{t != list2}

//emptiness
{list3: not empty| empty}

~vlist = (a2)
{ vlist }
{ LIST_ALL(vlist) }

range:{ LIST_RANGE(list2, 1, 2)}
{ LIST_RANGE(list2, a1, a3)}

subtract:{(a1,b1,c1) - (b1)}

~ SEED_RANDOM(10)
random:{LIST_RANDOM(t)}

listinc:{(a1) + 1}


