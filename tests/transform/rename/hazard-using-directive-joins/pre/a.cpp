int g(long x) { return (int)x; }
namespace a { int f(int x) { return x + 1; } }
using namespace a;
int u() { return g(1) + f(2); }
