namespace a { int f(int); }
namespace b { int f(int); }
#define BOTH(x) a::x(1) + b::x(2)
#define TWICE(x) a::x(1) + a::x(2)
int u1() { return BOTH(f); }
int u2() { return TWICE(f); }
