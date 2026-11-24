namespace a { int f(int x) { return x; } }
int h(int y = a::f(1)) { return y; }
struct S { int m = a::f(2); };
int u() { S s; return h() + s.m; }
