namespace acme { int f() { return 1; } int g(); }
int use() { return acme::f(); }
