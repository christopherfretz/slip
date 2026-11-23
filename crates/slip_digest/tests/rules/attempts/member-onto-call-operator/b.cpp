struct S { int call(int x) { return x; } };
int use(S s) { return s.call(1); }
