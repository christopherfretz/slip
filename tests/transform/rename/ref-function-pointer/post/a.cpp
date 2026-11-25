namespace a { int g(int x) { return x * 2; } }
int (*p)(int) = a::g;
int (&r)(int) = a::g;
int u() { return p(1) + r(2); }
