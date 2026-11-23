namespace acme { int f() { return 1; } int g() { return 2; } }
template <int (*F)()> struct P { int call() { return F(); } };
P<&acme::f> p;
int use() { return p.call() + P<&acme::g>().call(); }
