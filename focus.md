# add code insights to madr

next should
  1. Walk source nodes in document order (from source.fountain)
  2. For each, look up its structure UUID via source-structure.csv
  3. Check if that structure UUID appears in structure.fountain
  4. Return the first source node whose structure UUID is missing from fountain
  
show now shows line address for all trees

# repair
The find_children in show currently lists all descendants, not just direct children — that's something to refine. And the CSVS queries load all records then filter client-side, which could be smarter. 

# csvs 0.0.4
csvs now has commat prose inside it. this makes for a strange loopdeeloop since tsugiki prose right now has backlinks to csvs nodes.
it could be simpler to just have a prose segment per node, but that makes for worse ux with no single fountain file per book.
maybe building a prose file from segments is something that tsugiki can do as a composite view.

