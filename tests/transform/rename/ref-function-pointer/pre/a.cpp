namespace a { int f(int x) { return x * 2; } }
int (*p)(int) = a::f;
int (&r)(int) = a::f;
int u() { return p(1) + r(2); }
