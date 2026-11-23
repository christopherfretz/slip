namespace legacy { struct S {}; }
int g(legacy::S) { return 1; }
int f(int) { return 2; }
int use() { return g(legacy::S()) + f(1); }
