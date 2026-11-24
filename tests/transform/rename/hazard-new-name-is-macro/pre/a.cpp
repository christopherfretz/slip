#define LOG(x) (x)
namespace a { int f(int x) { return x; } }
int u() { return LOG(a::f(1)); }
