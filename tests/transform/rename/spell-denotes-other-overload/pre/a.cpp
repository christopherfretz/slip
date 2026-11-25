namespace a {
int f(long x) { return (int)x; }
namespace in {
int g(int x) { return x + 1; }
int u() { return f(1); }
}
}
