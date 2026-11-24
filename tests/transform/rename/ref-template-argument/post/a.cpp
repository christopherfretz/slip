namespace a { int g(int x) { return x; } }
template <int (*P)(int)> int call() { return P(1); }
int u() { return call<a::g>() + call<&a::g>(); }
