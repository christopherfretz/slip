#define ONCE(x) (x)
#define TWICE(x) ((x) + (x))
namespace a { int g(int x) { return x; } }
int u() { return ONCE(a::g(1)) + TWICE(a::g(2)); }
