namespace legacy { int f() { return 1; } int g() { return 2; } }
template <int (*F)()> struct P { int call() { return F(); } };
P<&legacy::f> p;
int use() { return p.call() + P<&legacy::g>().call(); }
