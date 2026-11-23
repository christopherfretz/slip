namespace acme { struct Thing {
#include "members.inc"
int v; }; }
int user(const acme::Thing& t) { return t.dep_get(); }
