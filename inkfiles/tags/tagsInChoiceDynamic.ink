VAR name = "Name"
// Should add tag 'tag Name' to choice at runtime
+ [Choice #tag {name}]
+ [Choice2 #tag 1 {name} 2 3 4]
+ [Choice #{name} tag 1 2 3 4]
->END