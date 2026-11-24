#define TWICE(x) (x(1) + x(2L))
namespace a {
int g(int x) { return x; }
int g(long x) { return (int)x + 1; }
int u() { return TWICE(g); }
}
