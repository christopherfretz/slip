namespace acme { struct S {}; }
int f(acme::S) { return 1; }
int f(int) { return 2; }
int use() { return f(acme::S()) + f(1); }
