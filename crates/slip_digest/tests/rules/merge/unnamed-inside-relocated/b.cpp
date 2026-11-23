namespace legacy { namespace { int hidden = 1; } int f() { return hidden; } }
int use() { return legacy::f(); }
