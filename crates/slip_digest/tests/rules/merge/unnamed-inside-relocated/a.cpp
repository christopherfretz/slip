namespace acme { namespace { int hidden = 1; } int f() { return hidden; } }
int use() { return acme::f(); }
