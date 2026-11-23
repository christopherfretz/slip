struct S { int operator()(int x) { return x; } };
int use(S s) { return s(1); }
