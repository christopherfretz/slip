namespace legacy { struct Thing {
#include "members.inc"
int v; }; }
int user(const legacy::Thing& t) { return t.dep_get(); }
