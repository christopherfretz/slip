#define ONCE(x) (x)
#define TWICE(x) ((x) + (x))
namespace a { int f(int x) { return x; } }
int u() { return ONCE(a::f(1)) + TWICE(a::f(2)); }
