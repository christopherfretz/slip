namespace a { extern "C" int f(int x) { return x; } }
int u() { return a::f(1); }
