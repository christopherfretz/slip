namespace { }
int g() { struct L { int f() { return 1; } }; return L().f(); }
int u() { return g(); }
