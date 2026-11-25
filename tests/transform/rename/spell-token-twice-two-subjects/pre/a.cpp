#define TWICE(x) (x(1) + x(2L))
namespace a {
int f(int x) { return x; }
int f(long x) { return (int)x + 1; }
int u() { return TWICE(f); }
}
