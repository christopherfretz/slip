namespace a { int f(int x) { return x; } }
template <int (*P)(int)> int call() { return P(1); }
int u() { return call<a::f>() + call<&a::f>(); }
