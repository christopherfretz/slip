namespace a { int g(int x) { return x; } }
int h(int y = a::g(1)) { return y; }
struct S { int m = a::g(2); };
int u() { S s; return h() + s.m; }
